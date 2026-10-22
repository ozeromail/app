package com.ozeromail.app

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.AccountKind
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.Endpoint
import uniffi.ozero_api.suggestedImapHost
import uniffi.ozero_api.suggestedSmtpHost

/**
 * A mail service with known servers, a page where its app passwords are made, and [hint]
 * on what making one takes.
 */
private enum class Provider(
    @StringRes val label: Int,
    val imap: String,
    val smtp: String,
    val smtpPort: Int,
    val appPasswords: String,
    @StringRes val hint: Int = R.string.signin_app_password_text,
) {
    // For those Google sign-in does not let in yet.
    GMAIL(
        R.string.signin_gmail,
        "imap.gmail.com",
        "smtp.gmail.com",
        Mail.SMTPS_PORT,
        "https://myaccount.google.com/apppasswords",
        R.string.signin_gmail_text,
    ),
    ICLOUD(R.string.signin_icloud, "imap.mail.me.com", "smtp.mail.me.com", 587, "https://account.apple.com/account/manage"),
    YAHOO(R.string.signin_yahoo, "imap.mail.yahoo.com", "smtp.mail.yahoo.com", Mail.SMTPS_PORT, "https://login.yahoo.com/account/security"),
    YANDEX(R.string.signin_yandex, "imap.yandex.com", "smtp.yandex.com", Mail.SMTPS_PORT, "https://id.yandex.ru/security/app-passwords"),
    MAILRU(R.string.signin_mailru, "imap.mail.ru", "smtp.mail.ru", Mail.SMTPS_PORT, "https://account.mail.ru/user/2-step-auth/passwords/"),
}

/** Where the person is: picking a service, or signing in to one; [provider] null is any IMAP server. */
private data class Step(val provider: Provider?) : java.io.Serializable

/**
 * Adds an account in two steps: first where the mail lives, then the sign-in for it. Google
 * goes straight to the browser; known services need only an email and an app password; any
 * other server shows its IMAP and SMTP fields. Everything to tap sits in the lower half.
 * With [onCancel], for a further account, system back and the back arrow leave it.
 * With [again], it signs in to that saved account again instead, keeping its mail.
 */
@Composable
fun SignInScreen(mail: Mail, onSignedIn: () -> Unit, onCancel: (() -> Unit)? = null, again: AccountItem? = null) {
    // Null while picking.
    var step by rememberSaveable { mutableStateOf<Step?>(null) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    // Not while signing in: leaving would cut it off halfway. From a sign-in, back goes to the list.
    fun go(to: Step?) {
        step = to
        error = null
    }
    BackHandler(enabled = busy || step != null) { if (!busy) go(null) }

    /** Runs a sign-in, busy meanwhile; on success leaves the screen. */
    suspend fun attempt(signIn: suspend () -> Unit) {
        busy = true
        error = null
        try {
            signIn()
            onSignedIn()
        } catch (e: CoreException) {
            error = e.text()
        } finally {
            busy = false
        }
    }

    LaunchedEffect(Unit) {
        for (answer in mail.googleRedirects) {
            // Back to the list, where Google is, from whatever step the person was on.
            go(null)
            attempt { mail.finishGoogle(answer, again) }
        }
    }

    fun open(url: String) {
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        } catch (e: ActivityNotFoundException) {
            error = R.string.signin_no_browser
        }
    }

    fun google() {
        error = null
        scope.launch {
            try {
                open(mail.startGoogle(again))
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    Column(
        Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .imePadding()
            .padding(horizontal = Gap.screen, vertical = 24.dp),
    ) {
        if (again != null) {
            Again(
                again,
                busy = busy,
                onCancel = onCancel,
                onGoogle = ::google,
                onSubmit = { password ->
                    busy = true
                    scope.launch { attempt { mail.signInAgain(again, password) } }
                },
                modifier = Modifier.weight(1f),
            )
        } else {
            AnimatedContent(
                step,
                Modifier.weight(1f),
                // Stable across process death, so each step's saved fields come back.
                contentKey = { it?.provider?.name ?: if (it == null) "pick" else "other" },
                transitionSpec = {
                    val move = Motion.calm<IntOffset>(Motion.SCREEN)
                    val fade = Motion.calm<Float>(Motion.SCREEN)
                    val forward = targetState != null
                    val sign = if (forward) 1 else -1
                    (slideInHorizontally(move) { sign * it / 3 } + fadeIn(fade)) togetherWith
                        (slideOutHorizontally(move) { -sign * it / 3 } + fadeOut(fade))
                },
                label = "step",
            ) { shown ->
                if (shown == null) {
                    Choose(
                        busy = busy,
                        onCancel = onCancel,
                        onGoogle = ::google,
                        onPick = { go(Step(it)) },
                    )
                } else {
                    Credentials(
                        shown.provider,
                        busy = busy,
                        onBack = { go(null) },
                        onAppPasswords = ::open,
                        onSubmit = { email, password, imap, smtp ->
                            // Set before the coroutine starts, so a second tap does nothing.
                            busy = true
                            scope.launch { attempt { mail.signIn(email, password, imap, smtp) } }
                        },
                    )
                }
            }
        }
        Reveal(error) { Text(stringResource(it), color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(top = 12.dp)) }
    }
}

/** The first step: where the mail lives, as a list low on the screen. */
@Composable
private fun Choose(busy: Boolean, onCancel: (() -> Unit)?, onGoogle: () -> Unit, onPick: (Provider?) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize()) {
        // Adding another account goes back to the mail; the arrow's row is there even without
        // it, so the title sits where the next step's does.
        Box(Modifier.padding(bottom = 12.dp).height(44.dp)) {
            if (onCancel != null) StepBack(stringResource(R.string.action_back), onCancel, busy)
        }
        LargeTitle(stringResource(R.string.signin_title))
        Text(
            stringResource(R.string.signin_text),
            color = colors.onSurfaceVariant,
            modifier = Modifier.padding(top = 6.dp),
        )
        Spacer(Modifier.weight(1f))
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(Radius.tile))
                .background(colors.surfaceContainer),
        ) {
            Choice(stringResource(R.string.signin_google), !busy, onGoogle, waiting = busy)
            // Any other server last.
            (Provider.entries + null).forEach { provider ->
                HorizontalDivider(Modifier.padding(start = Gap.card), color = colors.outlineVariant)
                Choice(stringResource(provider?.label ?: R.string.signin_other), !busy, onClick = { onPick(provider) })
            }
        }
    }
}

/**
 * Signing in to a saved account again: Google goes back to the browser, any other server
 * asks only for the new password, as its servers stay the same.
 */
@Composable
private fun Again(
    account: AccountItem,
    busy: Boolean,
    onCancel: (() -> Unit)?,
    onGoogle: () -> Unit,
    onSubmit: (password: String) -> Unit,
    modifier: Modifier,
) {
    val google = account.kind == AccountKind.GMAIL
    // Not saveable, as in [Credentials].
    var password by remember { mutableStateOf("") }
    val ready = !busy && (google || password.isNotEmpty())
    fun submit() {
        if (!ready) return
        if (google) onGoogle() else onSubmit(password)
    }
    Column(modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        // With the column's spacing, the title sits where the first step's does.
        Box(Modifier.height(44.dp)) {
            if (onCancel != null) StepBack(stringResource(R.string.action_back), onCancel, busy)
        }
        LargeTitle(stringResource(R.string.signin_again_title))
        Text(
            stringResource(if (google) R.string.signin_again_google else R.string.signin_again_password, account.email),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.weight(1f))
        if (!google) Field(password, { password = it }, R.string.signin_password, KeyboardType.Password, onDone = ::submit)
        Button(
            onClick = ::submit,
            enabled = ready,
            modifier = Modifier.fillMaxWidth().height(48.dp),
        ) {
            if (busy) {
                Spinner(LocalContentColor.current)
            } else {
                Text(stringResource(if (google) R.string.signin_again_with_google else R.string.signin_action))
            }
        }
    }
}

/** A row of the list of services, with a spinner while [waiting] on it. */
@Composable
private fun Choice(label: String, enabled: Boolean, onClick: () -> Unit, waiting: Boolean = false) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = Gap.card),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = Type.Row, modifier = Modifier.weight(1f))
        if (waiting) {
            Spinner(MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            Glyph(R.drawable.ic_next, null, MaterialTheme.colorScheme.onSurfaceVariant, size = 18.dp)
        }
    }
}

/**
 * The second step: an email and a password. A known service asks for an app password and
 * links to where it is made; any other server shows its IMAP and SMTP fields too.
 */
@Composable
private fun Credentials(
    provider: Provider?,
    busy: Boolean,
    onBack: () -> Unit,
    onAppPasswords: (String) -> Unit,
    onSubmit: (email: String, password: String, imap: Endpoint, smtp: Endpoint) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    var email by rememberSaveable { mutableStateOf("") }
    // Not saveable: the saved state is written outside the app's sandbox.
    var password by remember { mutableStateOf("") }
    // The server follows the email until the person types one of their own.
    var server by rememberSaveable { mutableStateOf<String?>(null) }
    var port by rememberSaveable { mutableStateOf(Mail.IMAPS_PORT.toString()) }
    var smtpServer by rememberSaveable { mutableStateOf<String?>(null) }
    var smtpPort by rememberSaveable { mutableStateOf(Mail.SMTPS_PORT.toString()) }

    // An address with a domain, which is also what the server is guessed from.
    val suggested = remember(email) { suggestedImapHost(email) }
    val suggestedSmtp = remember(email) { suggestedSmtpHost(email) }
    val host = (server ?: suggested ?: "").trim()
    val smtpHost = (smtpServer ?: suggestedSmtp ?: "").trim()
    val imap = provider?.let { Endpoint(it.imap, Mail.IMAPS_PORT.toUShort()) } ?: endpoint(host, port)
    val smtp = provider?.let { Endpoint(it.smtp, it.smtpPort.toUShort()) } ?: endpoint(smtpHost, smtpPort)
    val ready = !busy && suggested != null && password.isNotEmpty() && imap != null && smtp != null

    fun submit() {
        if (ready && imap != null && smtp != null) onSubmit(email.trim(), password, imap, smtp)
    }

    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        StepBack(stringResource(R.string.signin_title), onBack, busy)
        LargeTitle(stringResource(provider?.label ?: R.string.signin_other))
        if (provider != null) {
            Text(stringResource(provider.hint), color = colors.onSurfaceVariant)
            TextButton(
                onClick = { onAppPasswords(provider.appPasswords) },
                enabled = !busy,
                contentPadding = PaddingValues(0.dp),
            ) {
                Text(stringResource(R.string.signin_app_password_make))
            }
        }
        Spacer(Modifier.weight(1f))
        Field(email, { email = it }, R.string.signin_email, KeyboardType.Email)
        Field(
            password,
            { password = it },
            if (provider == null) R.string.signin_password else R.string.signin_app_password,
            KeyboardType.Password,
            onDone = if (provider == null) null else ::submit,
        )
        if (provider == null) {
            Server(host, { server = it }, R.string.signin_server, port, { port = it })
            Server(smtpHost, { smtpServer = it }, R.string.signin_smtp_server, smtpPort, { smtpPort = it }, onDone = ::submit)
        }
        Button(
            onClick = ::submit,
            enabled = ready,
            modifier = Modifier.fillMaxWidth().height(48.dp),
        ) {
            if (busy) {
                Spinner(LocalContentColor.current)
            } else {
                Text(stringResource(R.string.signin_action))
            }
        }
    }
}

private fun endpoint(host: String, port: String): Endpoint? {
    val number = port.toIntOrNull()?.takeIf { it in 1..65535 } ?: return null
    return if (host.isEmpty()) null else Endpoint(host, number.toUShort())
}

/** A server with its port on one line. */
@Composable
private fun Server(
    host: String,
    onHost: (String) -> Unit,
    @StringRes label: Int,
    port: String,
    onPort: (String) -> Unit,
    onDone: (() -> Unit)? = null,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Field(host, onHost, label, KeyboardType.Uri, Modifier.weight(1f))
        Field(port, onPort, R.string.signin_port, KeyboardType.Number, Modifier.weight(0.4f), onDone)
    }
}

@Composable
private fun Field(
    value: String,
    onChange: (String) -> Unit,
    @StringRes label: Int,
    keyboard: KeyboardType,
    modifier: Modifier = Modifier.fillMaxWidth(),
    onDone: (() -> Unit)? = null,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(stringResource(label)) },
        shape = RoundedCornerShape(Radius.medium),
        singleLine = true,
        visualTransformation = if (keyboard == KeyboardType.Password) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(
            keyboardType = keyboard,
            imeAction = if (onDone == null) ImeAction.Next else ImeAction.Done,
            autoCorrectEnabled = false,
        ),
        keyboardActions = KeyboardActions(onDone = { onDone?.invoke() }),
        modifier = modifier,
    )
}

/** A back arrow over a step's title, its glyph lined up with the title. */
@Composable
private fun StepBack(label: String, onBack: () -> Unit, busy: Boolean) {
    val back = 44.dp
    IconTile(R.drawable.ic_back, label, onBack, Modifier.offset(x = -(back - Gap.slot) / 2), size = back, enabled = !busy)
}
