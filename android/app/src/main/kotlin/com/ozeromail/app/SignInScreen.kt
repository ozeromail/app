package com.ozeromail.app

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalActivity
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
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
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.AccountKind
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.Endpoint
import uniffi.ozero_api.MailService
import uniffi.ozero_api.knownService
import uniffi.ozero_api.suggestedImapHost
import uniffi.ozero_api.suggestedSmtpHost

/**
 * A mail service with known servers, a page where its app passwords are made, [hint] on what
 * making one takes, and the [service] the core tells it by from an address.
 */
private enum class Provider(
    val imap: String,
    val smtp: String,
    val smtpPort: Int,
    val appPasswords: String,
    @StringRes val hint: Int = R.string.signin_app_password_text,
    val service: MailService? = null,
) {
    // For those Google sign-in does not let in yet; Gmail addresses go to Google sign-in.
    GMAIL(
        "imap.gmail.com",
        "smtp.gmail.com",
        Mail.SMTPS_PORT,
        "https://myaccount.google.com/apppasswords",
        R.string.signin_gmail_text,
    ),
    ICLOUD("imap.mail.me.com", "smtp.mail.me.com", 587, "https://account.apple.com/account/manage", service = MailService.I_CLOUD),
    YAHOO("imap.mail.yahoo.com", "smtp.mail.yahoo.com", Mail.SMTPS_PORT, "https://login.yahoo.com/account/security", service = MailService.YAHOO),
    YANDEX("imap.yandex.com", "smtp.yandex.com", Mail.SMTPS_PORT, "https://id.yandex.ru/security/app-passwords", service = MailService.YANDEX),
    MAILRU(
        "imap.mail.ru",
        "smtp.mail.ru",
        Mail.SMTPS_PORT,
        "https://account.mail.ru/user/2-step-auth/passwords/",
        service = MailService.MAIL_RU,
    ),
    ;

    companion object {
        /** The service an address is at, if sign-in knows it. */
        fun of(email: String): Provider? = knownService(email)?.let { service -> entries.firstOrNull { it.service == service } }
    }
}

/** Where the person is, past the first step. */
private sealed interface Step : java.io.Serializable {
    /** A known service, its address typed on the first step; for Gmail, on this one. */
    data class Known(val provider: Provider) : Step {
        val typed get() = provider == Provider.GMAIL
    }

    /** An address sign-in does not know: its servers are looked for. */
    data object Found : Step

    /** Servers typed, IMAP or Exchange; [lost] once none was found for the address. */
    data class Servers(val imap: Boolean, val lost: Boolean = false) : Step

    /** The ways in that take more than an address. */
    data object Manual : Step
}

/** How far a step is from the first, for which way the screens slide. */
private fun Step?.depth(): Int = when (this) {
    null -> 0
    Step.Found, Step.Manual -> 1
    is Step.Known -> if (typed) 2 else 1
    is Step.Servers -> 2
}

/**
 * Adds an account, starting from its address: Google signs in in the browser, a known service
 * asks for an app password, and any other address for its password, its servers then found
 * on their own; only when none is found are they typed. The first step is also the app's
 * first screen, with its name over it. Everything to tap sits in the lower half.
 * With [onCancel], for a further account, system back and the back arrow leave it.
 * With [again], it signs in to that saved account again instead, keeping its mail.
 */
@Composable
fun SignInScreen(mail: Mail, onSignedIn: () -> Unit, onCancel: (() -> Unit)? = null, again: AccountItem? = null) {
    // Null on the first step.
    var step by rememberSaveable { mutableStateOf<Step?>(null) }
    var email by rememberSaveable { mutableStateOf("") }
    // Not saveable: the saved state is written outside the app's sandbox.
    var password by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    // A password typed for one way in is not sent to another.
    fun go(to: Step?) {
        password = ""
        step = to
        error = null
    }
    fun back() = go(
        when (val at = step) {
            is Step.Known -> if (at.typed) Step.Manual else null
            is Step.Servers -> if (at.lost) null else Step.Manual
            else -> null
        },
    )
    // Not while signing in: leaving would cut it off halfway.
    BackHandler(enabled = busy || step != null) { if (!busy) back() }

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
            // Back to the first step, where Google is, from whatever step the person was on.
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
                open(mail.startGoogle(again, email.trim().takeIf { knownService(it) == MailService.GOOGLE }))
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    fun next() {
        if (busy || suggestedImapHost(email) == null) return
        if (knownService(email) == MailService.GOOGLE) google() else go(Provider.of(email)?.let { Step.Known(it) } ?: Step.Found)
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
                contentKey = {
                    when (it) {
                        null -> "start"
                        is Step.Known -> it.provider.name
                        Step.Found -> "found"
                        // One key while lost, so switching IMAP and Exchange does not slide.
                        is Step.Servers -> if (it.lost) "lost" else "servers-${it.imap}"
                        Step.Manual -> "manual"
                    }
                },
                transitionSpec = {
                    val move = Motion.calm<IntOffset>(Motion.SCREEN)
                    val fade = Motion.calm<Float>(Motion.SCREEN)
                    val sign = if (targetState.depth() >= initialState.depth()) 1 else -1
                    (slideInHorizontally(move) { sign * it / 3 } + fadeIn(fade)) togetherWith
                        (slideOutHorizontally(move) { -sign * it / 3 } + fadeOut(fade))
                },
                label = "step",
            ) { shown ->
                val lost = (shown as? Step.Servers)?.lost == true
                // Keeps the word that no server was found.
                val onKind = { imap: Boolean -> step = Step.Servers(imap, lost = true) }.takeIf { lost }
                when {
                    shown == null -> Start(
                        email,
                        { email = it },
                        busy = busy,
                        onCancel = onCancel,
                        onGoogle = ::google,
                        onNext = ::next,
                        onManual = { go(Step.Manual) },
                    )
                    shown == Step.Manual -> Manual(busy = busy, onBack = ::back, onPick = ::go)
                    shown is Step.Servers && !shown.imap -> Exchange(
                        email,
                        { email = it },
                        password,
                        { password = it },
                        typed = !lost,
                        onKind = onKind,
                        busy = busy,
                        // The server was not found: the fields to type it come out.
                        manual = lost || error == R.string.error_no_server || error == R.string.error_certificate_needed,
                        onBack = ::back,
                        onSubmit = { server, login, certificate ->
                            busy = true
                            scope.launch { attempt { mail.signInExchange(email.trim(), password, server, login, certificate) } }
                        },
                    )
                    else -> Credentials(
                        (shown as? Step.Known)?.provider,
                        email,
                        { email = it },
                        password,
                        { password = it },
                        typed = (shown as? Step.Known)?.typed == true || (shown is Step.Servers && !lost),
                        servers = shown is Step.Servers,
                        onKind = onKind,
                        busy = busy,
                        onBack = ::back,
                        onAppPasswords = ::open,
                        onSubmit = { imap, smtp ->
                            // Set before the coroutine starts, so a second tap does nothing.
                            busy = true
                            scope.launch {
                                attempt {
                                    if (imap != null && smtp != null) {
                                        mail.signIn(email.trim(), password, imap, smtp)
                                    } else {
                                        try {
                                            mail.signInFound(email.trim(), password)
                                        } catch (e: CoreException.NotFound) {
                                            // Typed then, IMAP first: the more common.
                                            step = Step.Servers(imap = true, lost = true)
                                            throw e
                                        }
                                    }
                                }
                            }
                        },
                    )
                }
            }
        }
        Reveal(error) { Text(stringResource(it), color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(top = 12.dp)) }
    }
}

/**
 * The first step, and the app's first screen: its name, then Google, or an address whose
 * service is told by its domain. For a further account, a back arrow and a title instead.
 */
@Composable
private fun Start(
    email: String,
    onEmail: (String) -> Unit,
    busy: Boolean,
    onCancel: (() -> Unit)?,
    onGoogle: () -> Unit,
    onNext: () -> Unit,
    onManual: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val google = remember(email) { knownService(email) == MailService.GOOGLE }
    val ready = !busy && remember(email) { suggestedImapHost(email) != null }
    Form(
        top = {
            if (onCancel != null) {
                StepBack(stringResource(R.string.action_back), onCancel, busy)
                LargeTitle(stringResource(R.string.signin_title))
            } else {
                Image(
                    painterResource(R.drawable.ozero_mark),
                    null,
                    Modifier.padding(top = 56.dp, bottom = 6.dp).size(76.dp),
                )
                Column {
                    LargeTitle(stringResource(R.string.app_name))
                    Text(stringResource(R.string.signin_welcome), style = Type.Writing, color = colors.onSurfaceVariant)
                }
            }
        },
        bottom = {
            SubmitButton(R.string.signin_google, busy = false, ready = !busy, onGoogle, quiet = true)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                HorizontalDivider(Modifier.weight(1f), color = colors.outlineVariant)
                Text(stringResource(R.string.signin_or), style = Type.Caption, color = colors.onSurfaceVariant)
                HorizontalDivider(Modifier.weight(1f), color = colors.outlineVariant)
            }
            Field(email, onEmail, R.string.signin_email, KeyboardType.Email, onDone = onNext)
            SubmitButton(if (google) R.string.signin_google else R.string.signin_next, busy, ready, onNext)
            TextButton(onClick = onManual, enabled = !busy, modifier = Modifier.align(Alignment.CenterHorizontally)) {
                Text(stringResource(R.string.signin_manual))
            }
            if (onCancel == null) {
                Text(
                    stringResource(R.string.signin_trust),
                    style = Type.Caption,
                    color = colors.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        },
    )
}

/** The ways in that take more than an address, as a list low on the screen. */
@Composable
private fun Manual(busy: Boolean, onBack: () -> Unit, onPick: (Step) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Form(
        top = {
            StepBack(stringResource(R.string.action_back), onBack, busy)
            LargeTitle(stringResource(R.string.signin_manual))
        },
        bottom = {
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(Radius.tile))
                    .background(colors.surfaceContainer),
            ) {
                Choice(stringResource(R.string.signin_exchange), !busy, onClick = { onPick(Step.Servers(imap = false)) })
                HorizontalDivider(Modifier.padding(start = Gap.card), color = colors.outlineVariant)
                Choice(stringResource(R.string.signin_other), !busy, onClick = { onPick(Step.Servers(imap = true)) })
                HorizontalDivider(Modifier.padding(start = Gap.card), color = colors.outlineVariant)
                Choice(stringResource(R.string.signin_gmail), !busy, onClick = { onPick(Step.Known(Provider.GMAIL)) })
            }
        },
    )
}

/** IMAP or Exchange, for servers typed once none was found. */
@Composable
private fun Kinds(imap: Boolean, enabled: Boolean, onPick: (imap: Boolean) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(Radius.medium))
            .background(colors.surfaceContainer)
            .padding(3.dp),
    ) {
        listOf(true to R.string.signin_imap, false to R.string.signin_exchange).forEach { (kind, label) ->
            val on = kind == imap
            Box(
                Modifier
                    .weight(1f)
                    .height(38.dp)
                    .clip(RoundedCornerShape(Radius.thumb))
                    .background(if (on) colors.surface else Color.Transparent)
                    .clickable(enabled = enabled && !on, role = Role.Tab) { onPick(kind) },
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    stringResource(label),
                    style = Type.Text.copy(fontWeight = Weight.SemiBold),
                    color = if (on) colors.onSurface else colors.onSurfaceVariant,
                )
            }
        }
    }
}

/** A step's title: what was picked when the address is [typed] here, else the address. */
@Composable
private fun StepTitle(typed: Boolean, @StringRes label: Int, email: String) {
    if (typed) LargeTitle(stringResource(label)) else Text(email.trim(), style = Type.Title)
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
    Form(
        modifier,
        top = {
            // With the column's spacing, the title sits where the first step's does.
            Box(Modifier.height(44.dp)) {
                if (onCancel != null) StepBack(stringResource(R.string.action_back), onCancel, busy)
            }
            LargeTitle(stringResource(R.string.signin_again_title))
            Text(
                stringResource(if (google) R.string.signin_again_google else R.string.signin_again_password, account.email),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
        bottom = {
            if (!google) Field(password, { password = it }, R.string.signin_password, KeyboardType.Password, onDone = ::submit)
            SubmitButton(if (google) R.string.signin_again_with_google else R.string.signin_action, busy, ready, ::submit)
        },
    )
}

/** A row of a list of ways in. */
@Composable
private fun Choice(label: String, enabled: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = Gap.card),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = Type.Row, modifier = Modifier.weight(1f))
        Glyph(R.drawable.ic_next, null, MaterialTheme.colorScheme.onSurfaceVariant, size = 18.dp)
    }
}

/**
 * A password for an address. A known service asks for an app password and links to where it
 * is made; with [servers], the IMAP and SMTP fields are typed too; with neither, the servers
 * are found from the address and [onSubmit] gets none. [typed] asks for the address here,
 * and [onKind] switches to Exchange once no server was found.
 */
@Composable
private fun Credentials(
    provider: Provider?,
    email: String,
    onEmail: (String) -> Unit,
    password: String,
    onPassword: (String) -> Unit,
    typed: Boolean,
    servers: Boolean,
    onKind: ((imap: Boolean) -> Unit)?,
    busy: Boolean,
    onBack: () -> Unit,
    onAppPasswords: (String) -> Unit,
    onSubmit: (imap: Endpoint?, smtp: Endpoint?) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
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
    val imap = provider?.let { Endpoint(it.imap, Mail.IMAPS_PORT.toUShort()) } ?: endpoint(host, port).takeIf { servers }
    val smtp = provider?.let { Endpoint(it.smtp, it.smtpPort.toUShort()) } ?: endpoint(smtpHost, smtpPort).takeIf { servers }
    val ready = !busy && suggested != null && password.isNotEmpty() && (!servers || (imap != null && smtp != null))

    fun submit() {
        if (ready) onSubmit(imap, smtp)
    }

    Form(
        top = {
            StepBack(stringResource(R.string.action_back), onBack, busy)
            StepTitle(typed, if (provider == null) R.string.signin_other else R.string.signin_gmail, email)
            if (provider != null) {
                Text(stringResource(provider.hint), color = colors.onSurfaceVariant)
                TextButton(
                    onClick = { onAppPasswords(provider.appPasswords) },
                    enabled = !busy,
                    contentPadding = PaddingValues(0.dp),
                ) {
                    Text(stringResource(R.string.signin_app_password_make))
                }
            } else if (!servers) {
                Text(stringResource(R.string.signin_found_text), color = colors.onSurfaceVariant)
            }
        },
        bottom = {
            onKind?.let { Kinds(imap = true, !busy, it) }
            if (typed) Field(email, onEmail, R.string.signin_email, KeyboardType.Email)
            Field(
                password,
                onPassword,
                if (provider == null) R.string.signin_password else R.string.signin_app_password,
                KeyboardType.Password,
                onDone = if (servers) null else ::submit,
            )
            // Finding the servers can take a while: say what is going on.
            if (provider == null && !servers) {
                Reveal(busy.takeIf { it }) {
                    Text(stringResource(R.string.signin_finding), color = colors.onSurfaceVariant)
                }
            }
            if (servers) {
                Server(host, { server = it }, R.string.signin_server, port, { port = it })
                Server(smtpHost, { smtpServer = it }, R.string.signin_smtp_server, smtpPort, { smtpPort = it }, onDone = ::submit)
            }
            SubmitButton(R.string.signin_action, busy, ready, ::submit)
        },
    )
}

/**
 * Exchange: a password, the server found from the address. The server and a login other
 * than the address can be typed too, and come out on their own when no server is found; so
 * can a certificate from the phone, for a server that asks for one. [typed] asks for the
 * address here, and [onKind] switches to IMAP once no server was found.
 */
@Composable
private fun Exchange(
    email: String,
    onEmail: (String) -> Unit,
    password: String,
    onPassword: (String) -> Unit,
    typed: Boolean,
    onKind: ((imap: Boolean) -> Unit)?,
    busy: Boolean,
    manual: Boolean,
    onBack: () -> Unit,
    onSubmit: (server: String?, login: String?, certificate: String?) -> Unit,
) {
    val context = LocalContext.current
    val activity = LocalActivity.current ?: context.activity()
    var server by rememberSaveable { mutableStateOf("") }
    var login by rememberSaveable { mutableStateOf("") }
    var certificate by rememberSaveable { mutableStateOf<String?>(null) }
    var typing by rememberSaveable { mutableStateOf(false) }
    // Once no server was found, it has to be typed, also after the error is gone.
    var needed by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(manual) {
        if (manual) needed = true
    }
    val shown = typing || manual || needed
    val ready = !busy && suggestedImapHost(email) != null && password.isNotEmpty() && (!(manual || needed) || server.isNotBlank())

    fun submit() {
        if (!ready) return
        val entered = { text: String -> text.trim().takeIf { shown && it.isNotEmpty() } }
        onSubmit(entered(server), entered(login), certificate.takeIf { shown })
    }

    Form(
        top = {
            StepBack(stringResource(R.string.action_back), onBack, busy)
            StepTitle(typed, R.string.signin_exchange, email)
            if (typed) Text(stringResource(R.string.signin_exchange_text), color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (!shown) {
                TextButton(onClick = { typing = true }, enabled = !busy, contentPadding = PaddingValues(0.dp)) {
                    Text(stringResource(R.string.signin_exchange_manual))
                }
            }
        },
        bottom = {
            onKind?.let { Kinds(imap = false, !busy, it) }
            if (typed) Field(email, onEmail, R.string.signin_email, KeyboardType.Email)
            Field(password, onPassword, R.string.signin_password, KeyboardType.Password, onDone = if (shown) null else ::submit)
            Reveal(shown.takeIf { it }) {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Field(server, { server = it }, R.string.signin_exchange_server, KeyboardType.Uri)
                    Field(login, { login = it }, R.string.signin_exchange_login, KeyboardType.Email, onDone = ::submit)
                    TextButton(
                        onClick = { activity?.let { chooseCertificate(it, certificate) { picked -> certificate = picked } } },
                        enabled = !busy && activity != null,
                        contentPadding = PaddingValues(0.dp),
                    ) {
                        Text(
                            certificate?.let { stringResource(R.string.signin_exchange_certificate, it) }
                                ?: stringResource(R.string.signin_exchange_certificate_choose),
                        )
                    }
                }
            }
            SubmitButton(R.string.signin_action, busy, ready, ::submit)
        },
    )
}

/**
 * A form: [top] at the top, [bottom] low on the screen where the thumb is. It scrolls when
 * the keyboard leaves too little room, and a tap outside the fields puts the keyboard away.
 */
@Composable
private fun Form(
    modifier: Modifier = Modifier,
    top: @Composable ColumnScope.() -> Unit,
    bottom: @Composable ColumnScope.() -> Unit,
) {
    val focus = LocalFocusManager.current
    BoxWithConstraints(modifier.fillMaxSize()) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .heightIn(min = maxHeight)
                .pointerInput(Unit) { detectTapGestures { focus.clearFocus() } },
            verticalArrangement = Arrangement.SpaceBetween,
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp), content = top)
            Column(Modifier.padding(top = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp), content = bottom)
        }
    }
}

/** The activity a context belongs to, through the wrappers around it. */
private tailrec fun Context.activity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.activity()
    else -> null
}

/** The full-width button that sends a form, with a spinner while it goes; [quiet] for a second way. */
@Composable
private fun SubmitButton(@StringRes label: Int, busy: Boolean, ready: Boolean, onClick: () -> Unit, quiet: Boolean = false) {
    val colors = MaterialTheme.colorScheme
    Button(
        onClick = onClick,
        enabled = ready,
        colors = if (quiet) {
            ButtonDefaults.buttonColors(containerColor = colors.surfaceContainer, contentColor = colors.onSurface)
        } else {
            ButtonDefaults.buttonColors()
        },
        modifier = Modifier.fillMaxWidth().height(48.dp),
    ) {
        if (busy) {
            Spinner(LocalContentColor.current)
        } else {
            Text(stringResource(label))
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
    val focus = LocalFocusManager.current
    // Autofill fills only the address and the password: a login or server it took for a
    // username would be filled, or emptied, over what was typed.
    val autofill = when (keyboard) {
        KeyboardType.Password -> ContentType.Password
        else -> if (label == R.string.signin_email) ContentType.Username else null
    }
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
        // Done always puts the keyboard away, also while the form is not ready to send.
        keyboardActions = KeyboardActions(onDone = {
            focus.clearFocus()
            onDone?.invoke()
        }),
        modifier = modifier.semantics { if (autofill != null) contentType = autofill },
    )
}

/** A back arrow over a step's title, its glyph lined up with the title. */
@Composable
private fun StepBack(label: String, onBack: () -> Unit, busy: Boolean) {
    val back = 44.dp
    IconTile(R.drawable.ic_back, label, onBack, Modifier.offset(x = -(back - Gap.slot) / 2), size = back, enabled = !busy)
}
