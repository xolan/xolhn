package dev.gpui.mobile;

import android.animation.Animator;
import android.animation.AnimatorListenerAdapter;
import android.animation.ObjectAnimator;
import android.animation.PropertyValuesHolder;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.media.AudioManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.SystemClock;
import android.util.Log;
import android.view.KeyEvent;
import android.view.View;
import android.view.WindowManager;
import android.view.animation.AccelerateInterpolator;

import androidx.core.graphics.Insets;
import androidx.core.splashscreen.SplashScreen;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;

/**
 * Custom Activity extending NativeActivity that integrates with the
 * AndroidX SplashScreen API.
 *
 * On API 31+ the system splash screen is displayed automatically via theme
 * attributes. On API 26-30 the AndroidX compat library emulates the same
 * behavior using the theme's windowBackground drawable.
 *
 * The splash screen is held visible until the Rust native library signals
 * that initialization is complete by setting NATIVE_INITIALIZED to true
 * (see src/android/jni.rs). This prevents the user from seeing an empty
 * or partially-rendered surface during startup.
 */
public class GpuiActivity extends GpuiInputActivity {

    /** Whether the native .so has been loaded via System.loadLibrary. */
    private static volatile boolean sNativeLibLoaded = false;

    private static final long MIN_SPLASH_DURATION_MS = 480L;
    private long mSplashStartUptimeMs = 0L;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Install the splash screen BEFORE calling super.onCreate().
        // This is required by the AndroidX SplashScreen API.
        SplashScreen splash = SplashScreen.installSplashScreen(this);
        mSplashStartUptimeMs = SystemClock.uptimeMillis();

        // NativeActivity loads the .so via dlopen (loadNativeCode), which does
        // NOT register JNI symbols with the classloader. We must call
        // System.loadLibrary() ourselves so that JNI can resolve our native
        // methods. Reading the library name from the manifest meta-data ensures
        // we stay in sync with the nativeLibraryName placeholder.
        if (!sNativeLibLoaded) {
            try {
                ActivityInfo ai = getPackageManager().getActivityInfo(
                        getComponentName(), PackageManager.GET_META_DATA);
                String libName = ai.metaData.getString("android.app.lib_name");
                if (libName != null) {
                    System.loadLibrary(libName);
                    sNativeLibLoaded = true;
                }
            } catch (PackageManager.NameNotFoundException e) {
                // Shouldn't happen — we're querying our own activity.
            } catch (UnsatisfiedLinkError e) {
                // Library may already be loaded by NativeActivity; that's fine.
                sNativeLibLoaded = true;
            }
        }

        // Keep the splash screen visible until the native side is ready and the boot X-draw finishes.
        splash.setKeepOnScreenCondition(() ->
                !isNativeReady() || (SystemClock.uptimeMillis() - mSplashStartUptimeMs) < MIN_SPLASH_DURATION_MS);

        splash.setOnExitAnimationListener(splashScreenView -> {
            View rootView = splashScreenView.getView();
            try {
                View iconView = splashScreenView.getIconView();
                if (iconView != null) {
                    ObjectAnimator iconAnim = ObjectAnimator.ofPropertyValuesHolder(
                            iconView,
                            PropertyValuesHolder.ofFloat(View.SCALE_X, 1f, 1.12f),
                            PropertyValuesHolder.ofFloat(View.SCALE_Y, 1f, 1.12f),
                            PropertyValuesHolder.ofFloat(View.ALPHA, 1f, 0f));
                    iconAnim.setInterpolator(new AccelerateInterpolator());
                    iconAnim.setDuration(200L);
                    iconAnim.start();
                }
            } catch (Exception ignored) {
                // Guard against OEM / relaunch SplashScreenView.getIconView() quirks.
            }

            ObjectAnimator fadeOut = ObjectAnimator.ofFloat(rootView, View.ALPHA, 1f, 0f);
            fadeOut.setInterpolator(new AccelerateInterpolator());
            fadeOut.setDuration(200L);
            fadeOut.addListener(new AnimatorListenerAdapter() {
                @Override
                public void onAnimationEnd(Animator animation) {
                    splashScreenView.remove();
                }

                @Override
                public void onAnimationCancel(Animator animation) {
                    splashScreenView.remove();
                }
            });
            fadeOut.start();
        });

        // Route volume keys to the MUSIC stream so they control media volume
        // rather than the ringer/notification volume.
        setVolumeControlStream(AudioManager.STREAM_MUSIC);

        super.onCreate(savedInstanceState);

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            WindowManager.LayoutParams lp = getWindow().getAttributes();
            lp.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS;
            getWindow().setAttributes(lp);
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams lp = getWindow().getAttributes();
            lp.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(lp);
        }

        View decorView = getWindow().getDecorView();
        ViewCompat.setOnApplyWindowInsetsListener(decorView, (view, windowInsets) -> {
            dispatchSafeAreaInsets(windowInsets);
            return ViewCompat.onApplyWindowInsets(view, windowInsets);
        });
        WindowInsetsCompat initialInsets = ViewCompat.getRootWindowInsets(decorView);
        if (initialInsets != null) {
            dispatchSafeAreaInsets(initialInsets);
        }
        ViewCompat.requestApplyInsets(decorView);
    }

    @Override
    protected void onResume() {
        super.onResume();
        View decorView = getWindow().getDecorView();
        WindowInsetsCompat insets = ViewCompat.getRootWindowInsets(decorView);
        if (insets != null) {
            dispatchSafeAreaInsets(insets);
        }
        ViewCompat.requestApplyInsets(decorView);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            View decorView = getWindow().getDecorView();
            WindowInsetsCompat insets = ViewCompat.getRootWindowInsets(decorView);
            if (insets != null) {
                dispatchSafeAreaInsets(insets);
            }
        }
    }

    private void dispatchSafeAreaInsets(WindowInsetsCompat windowInsets) {
        Insets safe = windowInsets.getInsets(
                WindowInsetsCompat.Type.systemBars()
                        | WindowInsetsCompat.Type.displayCutout());
        if (!sNativeLibLoaded) {
            return;
        }
        try {
            nativeOnSafeAreaInsets(safe.top, safe.bottom, safe.left, safe.right);
        } catch (UnsatisfiedLinkError e) {
            Log.w("GpuiActivity", "nativeOnSafeAreaInsets not available yet");
        }
    }

    /**
     * Check if the native library is fully initialized.
     * Returns false if the .so hasn't been loaded yet or if
     * NATIVE_INITIALIZED hasn't been set to true.
     */
    private boolean isNativeReady() {
        if (!sNativeLibLoaded) {
            return false;
        }
        try {
            return nativeIsInitialized();
        } catch (UnsatisfiedLinkError e) {
            return false;
        }
    }

    /**
     * Intercept key events to handle volume and media buttons.
     */
    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        int keyCode = event.getKeyCode();
        switch (keyCode) {
            case KeyEvent.KEYCODE_VOLUME_UP:
            case KeyEvent.KEYCODE_VOLUME_DOWN:
            case KeyEvent.KEYCODE_VOLUME_MUTE:
            case KeyEvent.KEYCODE_MEDIA_PLAY:
            case KeyEvent.KEYCODE_MEDIA_PAUSE:
            case KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE:
            case KeyEvent.KEYCODE_MEDIA_NEXT:
            case KeyEvent.KEYCODE_MEDIA_PREVIOUS:
            case KeyEvent.KEYCODE_MEDIA_STOP:
            case KeyEvent.KEYCODE_HEADSETHOOK:
                return super.dispatchKeyEvent(event);

            default:
                return super.dispatchKeyEvent(event);
        }
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
    }

    /**
     * Handle new intents delivered to this singleTask activity.
     */
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);

        Uri data = intent.getData();
        if (data != null) {
            String url = data.toString();
            Log.i("GpuiActivity", "onNewIntent deeplink: " + url);
            try {
                nativeOnDeepLink(url);
            } catch (UnsatisfiedLinkError e) {
                Log.w("GpuiActivity", "nativeOnDeepLink not available yet");
            }
        }
    }

    /**
     * JNI bridge to check if the Rust NATIVE_INITIALIZED flag is set.
     */
    private static native boolean nativeIsInitialized();

    /**
     * JNI bridge to notify Rust of an incoming deeplink URL.
     */
    private static native void nativeOnDeepLink(String url);

    /**
     * JNI bridge to forward system bar and display cutout safe-area insets in physical pixels.
     */
    private static native void nativeOnSafeAreaInsets(int top, int bottom, int left, int right);
}
