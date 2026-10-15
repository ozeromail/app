package com.ozeromail.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.annotation.StringRes
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
        setContent {
            OzeroTheme {
                Surface(Modifier.fillMaxSize()) {
                    App(mail)
                }
            }
        }
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
    when (val s = start) {
        Start.Loading -> Unit
        Start.SignIn -> SignInScreen(mail, onSignedIn = { start = Start.Inbox })
        Start.Inbox -> InboxScreen(mail)
        is Start.Failed -> Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
            Text(stringResource(s.text))
        }
    }
}
