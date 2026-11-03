package pl.prywatne.tablicaofert

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject

/**
 * "Share" from the browser → Tablica Ofert: the shared link or text goes to the app's
 * "Add offer" window (zalozenia.md §3). Nothing is sent anywhere - the person saves it.
 */
class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  private var pending: String? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    // System bars sit on the app's yellow sign colour (--signal), so their icons stay dark.
    val clear = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
    enableEdgeToEdge(statusBarStyle = clear, navigationBarStyle = clear)
    super.onCreate(savedInstanceState)
    window.decorView.setBackgroundColor(0xFFFFC61A.toInt())
    // Edge-to-edge (forced from Android 15): keep the page clear of the status bar,
    // navigation bar, camera cutout and the on-screen keyboard. The WebView is added to
    // this container later, so the padding goes on the container, not on the WebView.
    ViewCompat.setOnApplyWindowInsetsListener(findViewById<View>(android.R.id.content)) { v, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      v.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      WindowInsetsCompat.CONSUMED
    }
    pending = sharedText(intent)
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    // Cold start: the page needs a moment to load before it can take the text.
    pending?.let { text -> webView.postDelayed({ deliver(text) }, 1500) }
    pending = null
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    sharedText(intent)?.let { deliver(it) }
  }

  private fun sharedText(intent: Intent?): String? {
    if (intent?.action != Intent.ACTION_SEND || intent.type?.startsWith("text/") != true) return null
    val subject = intent.getStringExtra(Intent.EXTRA_SUBJECT).orEmpty()
    val text = intent.getStringExtra(Intent.EXTRA_TEXT).orEmpty()
    return listOf(subject, text).filter { it.isNotBlank() }.joinToString("\n").take(100_000).ifBlank { null }
  }

  private fun deliver(text: String) {
    // JSONObject.quote makes the text a safe JavaScript string literal.
    val literal = JSONObject.quote(text)
    webView?.evaluateJavascript(
      "(function(t){window.__tablicaShared=t;window.dispatchEvent(new CustomEvent('tablica-share',{detail:t}));})($literal)",
      null,
    )
  }
}
