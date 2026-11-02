package com.ferrowl.app.ui.start

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle

@Composable
fun StartScreen(vm: StartViewModel, onDecided: (hasAccount: Boolean) -> Unit) {
    val hasAccount by vm.hasAccount.collectAsStateWithLifecycle()
    LaunchedEffect(hasAccount) { hasAccount?.let(onDecided) }
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}
