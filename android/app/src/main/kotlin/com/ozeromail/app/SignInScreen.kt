package com.ozeromail.app

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.Endpoint
import uniffi.ozero_api.suggestedImapHost
import uniffi.ozero_api.suggestedSmtpHost

/**
 * Adds a Gmail account through Google in the browser, or an IMAP account with its SMTP server.
 * The fields sit in the lower half, in reach of the thumb.
 * With [onCancel], for a further account, system back and Cancel leave it.
 */
@Composable
fun SignInScreen(mail: Mail, onSignedIn: () -> Unit, onCancel: (() -> Unit)? = null) {

    var email by rememberSaveable { mutableStateOf("") }
    // Not saveable: the saved state is written outside the app's sandbox.
    var password by remember { mutableStateOf("") }
    // The server follows the email until the person types one of their own.
    var server by rememberSaveable { mutableStateOf<String?>(null) }
    var port by rememberSaveable { mutableStateOf(Mail.IMAPS_PORT.toString()) }
    var smtpServer by rememberSaveable { mutableStateOf<String?>(null) }
    var smtpPort by rememberSaveable { mutableStateOf(Mail.SMTPS_PORT.toString()) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    // Not while signing in: leaving would cut it off halfway.
    if (onCancel != null) BackHandler(enabled = !busy, onBack = onCancel)

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
        for (answer in mail.googleRedirects) attempt { mail.finishGoogle(answer) }
    }

    fun google() {
        error = null
        scope.launch {
            try {
                context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(mail.startGoogle())))
            } catch (e: ActivityNotFoundException) {
                error = R.string.signin_no_browser
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    val suggested = remember(email) { suggestedImapHost(email) }
    val suggestedSmtp = remember(email) { suggestedSmtpHost(email) }
    val host = (server ?: suggested ?: "").trim()
    val smtpHost = (smtpServer ?: suggestedSmtp ?: "").trim()
    val imap = endpoint(host, port)
    val smtp = endpoint(smtpHost, smtpPort)
    val ready = !busy && suggested != null && password.isNotEmpty() && imap != null && smtp != null

    fun submit() {
        if (!ready || imap == null || smtp == null) return
        // Set before the coroutine starts, so a second tap does nothing.
        busy = true
        scope.launch { attempt { mail.signIn(email.trim(), password, imap, smtp) } }
    }

    Column(
        Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .imePadding()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            LargeTitle(stringResource(R.string.signin_title), Modifier.weight(1f))
            if (onCancel != null) {
                TextButton(onClick = onCancel, enabled = !busy) { Text(stringResource(R.string.action_cancel)) }
            }
        }
        Spacer(Modifier.weight(1f))
        OutlinedButton(onClick = ::google, enabled = !busy, modifier = Modifier.fillMaxWidth().height(48.dp)) {
            Text(stringResource(R.string.signin_google))
        }
        Field(email, { email = it }, R.string.signin_email, KeyboardType.Email)
        Field(password, { password = it }, R.string.signin_password, KeyboardType.Password)
        Server(host, { server = it }, R.string.signin_server, port, { port = it })
        Server(smtpHost, { smtpServer = it }, R.string.signin_smtp_server, smtpPort, { smtpPort = it }, onDone = ::submit)
        error?.let { Text(stringResource(it), color = MaterialTheme.colorScheme.error) }
        Button(
            onClick = ::submit,
            enabled = ready,
            modifier = Modifier.fillMaxWidth().height(48.dp),
        ) {
            Text(stringResource(R.string.signin_action))
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
        shape = RoundedCornerShape(14.dp),
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
