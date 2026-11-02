package com.ferrowl.app

import android.app.Application
import dagger.hilt.android.HiltAndroidApp

/** Application entry point. `@HiltAndroidApp` starts dependency injection. */
@HiltAndroidApp
class FerrowlApp : Application()
