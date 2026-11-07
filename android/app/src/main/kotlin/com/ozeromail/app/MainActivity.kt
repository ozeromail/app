package com.ozeromail.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.annotation.StringRes
import androidx.compose.animation.Crossfade
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import uniffi.ozero_api.CoreException

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val mail = (application as OzeroApp).mail
        // Only a fresh launch: a recreated activity keeps the intent it was first started with.
        if (savedInstanceState == null) handle(intent)
        setContent {
            // The activity outlives a change of the system theme, so the bars' icons follow it here.
            val dark = isSystemInDarkTheme()
            LaunchedEffect(dark) { enableEdgeToEdge() }
            OzeroTheme {
                Surface(Modifier.fillMaxSize()) {
                    ProvideRoom { App(mail) }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    /** Google sign-in coming back, or a message another app asks for. */
    private fun handle(intent: Intent) {
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return
        val mail = (application as OzeroApp).mail
        val uri = intent.data
        if (uri?.scheme == packageName) mail.googleRedirects.trySend(uri.toString()) else sharedOf(intent)?.let { mail.shared.trySend(it) }
    }
}

private sealed interface Start {
    data object Loading : Start
    data object SignIn : Start
    data object Inbox : Start
    data class Failed(@StringRes val text: Int) : Start
}

@Composable
private fun App(mail: Mail) {
    var start by remember { mutableStateOf<Start>(Start.Loading) }
    LaunchedEffect(Unit) {
        start = try {
            if (mail.hasAccounts()) Start.Inbox else Start.SignIn
        } catch (e: CoreException) {
            Start.Failed(e.text())
        }
    }
    // The first screen fades in, as does the inbox after signing in.
    Crossfade(start, animationSpec = Motion.calm(Motion.NOTE), label = "start") { s ->
        when (s) {
            Start.Loading -> Unit
            Start.SignIn -> Middle(LIST_MAX, Modifier.fillMaxSize()) {
                SignInScreen(mail, onSignedIn = { start = Start.Inbox })
            }
            Start.Inbox -> InboxScreen(mail, onNoAccounts = { start = Start.SignIn })
            is Start.Failed -> Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                Text(stringResource(s.text))
            }
        }
    }
}
