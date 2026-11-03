package com.ferrowl.app

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/**
 * Receives the browser coming back from Microsoft's sign-in page
 * (`msauth://com.ferrowl.app/...`) and hands the address to [MainActivity].
 *
 * A separate, invisible activity is needed because the browser may open it
 * in its own task: forwarding with NEW_TASK + CLEAR_TOP brings back the
 * app's task with the existing MainActivity (closing the Custom Tab above
 * it), or starts it again if Android had stopped the app.
 */
class OAuthRedirectActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        startActivity(
            Intent(this, MainActivity::class.java)
                .setData(intent?.data)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
        )
        finish()
    }
}
