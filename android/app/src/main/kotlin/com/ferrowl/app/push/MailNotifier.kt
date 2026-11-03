package com.ferrowl.app.push

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.ferrowl.app.MainActivity
import com.ferrowl.app.R
import com.ferrowl.core.Header
import com.ferrowl.core.translate
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

/** Shows the "new mail" notifications (one, replaced at each update). */
@Singleton
class MailNotifier @Inject constructor(@ApplicationContext private val context: Context) {

    /** One message: sender and subject; several: how many, and the newest. */
    fun show(fresh: List<Header>) {
        val newest = fresh.first()
        val builder = base()
        if (fresh.size == 1) {
            builder.setContentTitle(newest.from).setContentText(newest.subject)
        } else {
            val style = NotificationCompat.InboxStyle()
            fresh.take(5).forEach { style.addLine("${it.from}: ${it.subject}") }
            builder
                .setContentTitle(translate("notif_many").replace("{n}", fresh.size.toString()))
                .setContentText("${newest.from}: ${newest.subject}")
                .setStyle(style)
                .setNumber(fresh.size)
        }
        post(builder)
    }

    /** When the inbox could not be read: just "New mail". */
    fun showGeneric() = post(base().setContentTitle(translate("notif_new_mail")))

    private fun base(): NotificationCompat.Builder {
        ensureChannel()
        val open = PendingIntent.getActivity(
            context,
            0,
            Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setCategory(NotificationCompat.CATEGORY_EMAIL)
            .setContentIntent(open)
            .setAutoCancel(true)
    }

    private fun post(builder: NotificationCompat.Builder) {
        // Android 13+: nothing to show without the user's permission.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        NotificationManagerCompat.from(context).notify(ID, builder.build())
    }

    private fun ensureChannel() {
        val manager = context.getSystemService(NotificationManager::class.java)
        if (manager.getNotificationChannel(CHANNEL) == null) {
            manager.createNotificationChannel(
                NotificationChannel(CHANNEL, translate("notif_channel"), NotificationManager.IMPORTANCE_DEFAULT),
            )
        }
    }

    private companion object {
        const val CHANNEL = "new_mail"
        const val ID = 1
    }
}
