package com.ferrowl.app.ui.start

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

/** Checks once whether an account is already set up. */
@HiltViewModel
class StartViewModel @Inject constructor(repo: MailRepository) : ViewModel() {
    private val _hasAccount = MutableStateFlow<Boolean?>(null)

    /** null while checking, then true/false. */
    val hasAccount: StateFlow<Boolean?> = _hasAccount.asStateFlow()

    init {
        viewModelScope.launch { _hasAccount.value = repo.savedAccount() != null }
    }
}
