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
    // A share is taken once: not again when Android re-creates the app (after it was closed in the
    // background, or a change of the system's text size) or opens it from the recent apps.
    val fromHistory = (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0
    if (savedInstanceState == null && !fromHistory) pending = sharedText(intent)
    if (intent?.action == Intent.ACTION_SEND) intent = Intent(Intent.ACTION_MAIN)
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    pending?.let { deliver(it) }
    pending = null
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    val text = sharedText(intent) ?: return
    // A second share while the app is still starting waits like the first one.
    if (webView == null) pending = text else deliver(text)
  }

  private fun sharedText(intent: Intent?): String? {
    if (intent?.action != Intent.ACTION_SEND || intent.type?.startsWith("text/") != true) return null
    val subject = intent.getCharSequenceExtra(Intent.EXTRA_SUBJECT)?.toString().orEmpty()
    // A styled text (CharSequence) is not a String extra - read both the same way.
    val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString().orEmpty()
    return listOf(subject, text).filter { it.isNotBlank() }.joinToString("\n").take(100_000).ifBlank { null }
  }

  /**
   * Hands the text to the page once the app listens (`__tablicaReady`); until then - the page still
   * loading on a cold start or a slow phone - it tries again every 0.4 s, for up to 20 s.
   */
  private fun deliver(text: String, tries: Int = 50) {
    // JSONObject.quote makes the text a safe JavaScript string literal.
    val literal = JSONObject.quote(text)
    val view = webView ?: return
    view.evaluateJavascript(
      "(function(t){if(!window.__tablicaReady)return false;window.dispatchEvent(new CustomEvent('tablica-share',{detail:t}));return true;})($literal)",
    ) { result ->
      if (result != "true" && tries > 1) view.postDelayed({ deliver(text, tries - 1) }, 400)
    }
  }
}
