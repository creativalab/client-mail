package com.ferrowl.app.di

import android.content.Context
import com.ferrowl.app.data.KeystoreTokenVault
import com.ferrowl.core.Engine
import com.ferrowl.core.setTokenVault
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import java.util.Locale
import javax.inject.Singleton

/** How Hilt builds objects that are not ours (here: the Rust engine). */
@Module
@InstallIn(SingletonComponent::class)
object AppModule {
    /**
     * One engine per app process, with its files in the app's private
     * storage. Sign-in tokens go to the Keystore-backed vault.
     */
    @Provides
    @Singleton
    fun engine(@ApplicationContext context: Context, vault: KeystoreTokenVault): Engine {
        setTokenVault(vault)
        return Engine(dataDir = context.filesDir.resolve("mail").path, language = Locale.getDefault().language)
    }
}
