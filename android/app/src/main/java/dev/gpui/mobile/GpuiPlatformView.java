package dev.gpui.mobile;

import android.app.Activity;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.WebChromeClient;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import java.util.HashMap;
import java.util.Map;

/**
 * Helper class for managing native platform views embedded in the GPUI render tree.
 *
 * Platform views are native Android Views positioned absolutely over the
 * NativeActivity's content area. GPUI controls their position and visibility
 * via JNI calls from Rust.
 */
public class GpuiPlatformView {
    private static final String TAG = "GpuiPlatformView";
    private static final Handler mainHandler = new Handler(Looper.getMainLooper());

    /** Map of view ID -> native View */
    private static final Map<Long, View> views = new HashMap<>();

    /** Map of view ID -> FrameLayout container */
    private static final Map<Long, FrameLayout> containers = new HashMap<>();

    /** The root FrameLayout that hosts all platform views */
    private static FrameLayout rootContainer;

    private static void ensureRootContainer(Activity activity) {
        if (rootContainer != null) {
            return;
        }

        mainHandler.post(() -> {
            if (rootContainer != null) return;

            rootContainer = new FrameLayout(activity);
            rootContainer.setLayoutParams(new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            ));

            activity.addContentView(rootContainer, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            ));

            try {
                View decorView = activity.getWindow().getDecorView();
                logViewHierarchy(decorView, 0);
            } catch (Exception e) {
                Log.w(TAG, "Could not log view hierarchy: " + e.getMessage());
            }

            Log.i(TAG, "Root container created and added to activity");
        });
    }

    public static boolean createView(
            Activity activity,
            String viewType,
            long viewId,
            float x, float y,
            float width, float height,
            String creationParams) {

        Log.i(TAG, "createView: type=" + viewType + " id=" + viewId
                + " bounds=(" + x + ", " + y + ", " + width + ", " + height + ")"
                + " params=" + creationParams);

        ensureRootContainer(activity);

        Map<String, String> params = parseCreationParams(creationParams);

        mainHandler.post(() -> {
            try {
                float density = activity.getResources().getDisplayMetrics().density;

                FrameLayout container = new FrameLayout(activity);
                FrameLayout.LayoutParams layoutParams = new FrameLayout.LayoutParams(
                    (int)(width * density),
                    (int)(height * density)
                );
                layoutParams.leftMargin = (int)(x * density);
                layoutParams.topMargin = (int)(y * density);
                container.setLayoutParams(layoutParams);

                View view = createViewForType(activity, viewType, params);
                if (view != null) {
                    container.addView(view, new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT
                    ));
                }

                views.put(viewId, view != null ? view : container);
                containers.put(viewId, container);

                if (rootContainer != null) {
                    rootContainer.addView(container);
                }

                Log.i(TAG, "View created: id=" + viewId + " type=" + viewType);
            } catch (Exception e) {
                Log.e(TAG, "Failed to create view: " + e.getMessage(), e);
            }
        });

        return true;
    }

    private static Map<String, String> parseCreationParams(String params) {
        Map<String, String> map = new HashMap<>();
        if (params == null || params.isEmpty()) return map;
        for (String pair : params.split("\\|")) {
            int eq = pair.indexOf('=');
            if (eq > 0) {
                map.put(pair.substring(0, eq), pair.substring(eq + 1));
            }
        }
        return map;
    }

    private static View createViewForType(Activity activity, String viewType, Map<String, String> params) {
        switch (viewType) {
            case "container":
                FrameLayout frame = new FrameLayout(activity);
                frame.setBackgroundColor(0x00000000);
                return frame;

            case "webview":
                return createWebViewView(activity, params);

            case "map":
                FrameLayout mapContainer = new FrameLayout(activity);
                mapContainer.setBackgroundColor(0xFFE0E0E0);
                return mapContainer;

            default:
                Log.w(TAG, "Unsupported view type: " + viewType + ", creating empty container");
                return new FrameLayout(activity);
        }
    }

    private static View createWebViewView(Activity activity, Map<String, String> params) {
        boolean jsEnabled = Boolean.parseBoolean(params.getOrDefault("javascript_enabled", "true"));
        boolean domStorage = Boolean.parseBoolean(params.getOrDefault("dom_storage_enabled", "true"));
        boolean zoom = Boolean.parseBoolean(params.getOrDefault("zoom_enabled", "true"));
        String url = params.getOrDefault("url", "");
        String html = params.getOrDefault("html", "");

        WebView wv = new WebView(activity);
        WebSettings settings = wv.getSettings();
        settings.setJavaScriptEnabled(jsEnabled);
        settings.setDomStorageEnabled(domStorage);
        settings.setBuiltInZoomControls(zoom);
        if (zoom) settings.setDisplayZoomControls(false);
        settings.setLoadWithOverviewMode(true);
        settings.setUseWideViewPort(true);

        wv.setWebViewClient(new WebViewClient());
        wv.setWebChromeClient(new WebChromeClient());

        if (!html.isEmpty()) {
            wv.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null);
        } else if (!url.isEmpty()) {
            wv.loadUrl(url);
        }

        return wv;
    }

    public static void setBounds(long viewId, float x, float y, float width, float height) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container == null) {
                Log.w(TAG, "setBounds: no container for id=" + viewId);
                return;
            }

            Activity activity = (Activity) container.getContext();
            float density = activity.getResources().getDisplayMetrics().density;

            FrameLayout.LayoutParams params = (FrameLayout.LayoutParams) container.getLayoutParams();
            params.leftMargin = (int)(x * density);
            params.topMargin = (int)(y * density);
            params.width = (int)(width * density);
            params.height = (int)(height * density);
            container.setLayoutParams(params);
        });
    }

    public static void setVisible(long viewId, boolean visible) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container == null) return;
            container.setVisibility(visible ? View.VISIBLE : View.GONE);
        });
    }

    public static void setZIndex(long viewId, int zIndex) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container == null) return;
            container.setZ(zIndex);
        });
    }

    public static void disposeView(long viewId) {
        mainHandler.post(() -> {
            FrameLayout container = containers.get(viewId);
            if (container != null) {
                if (rootContainer != null) {
                    rootContainer.removeView(container);
                }
                container.removeAllViews();
                containers.remove(viewId);
            }
            views.remove(viewId);
            Log.i(TAG, "View disposed: id=" + viewId);
        });
    }

    public static View getView(long viewId) {
        return views.get(viewId);
    }

    public static FrameLayout getContainer(long viewId) {
        return containers.get(viewId);
    }

    public static void pauseAll() {
        mainHandler.post(() -> {
            for (FrameLayout container : containers.values()) {
                container.setVisibility(View.INVISIBLE);
            }
            Log.i(TAG, "All platform views paused");
        });
    }

    public static void resumeAll() {
        mainHandler.post(() -> {
            for (FrameLayout container : containers.values()) {
                container.setVisibility(View.VISIBLE);
            }
            Log.i(TAG, "All platform views resumed");
        });
    }

    public static void disposeAll() {
        mainHandler.post(() -> {
            for (FrameLayout container : containers.values()) {
                if (rootContainer != null) {
                    rootContainer.removeView(container);
                }
                container.removeAllViews();
            }
            views.clear();
            containers.clear();
            Log.i(TAG, "All platform views disposed");
        });
    }

    public static boolean hitTest(float x, float y) {
        for (Map.Entry<Long, FrameLayout> entry : containers.entrySet()) {
            FrameLayout container = entry.getValue();
            if (container.getVisibility() != View.VISIBLE) continue;

            int[] location = new int[2];
            container.getLocationOnScreen(location);
            int left = location[0];
            int top = location[1];
            int right = left + container.getWidth();
            int bottom = top + container.getHeight();

            if (x >= left && x <= right && y >= top && y <= bottom) {
                return true;
            }
        }
        return false;
    }

    public static void dispatchTouch(float x, float y, int action) {
        mainHandler.post(() -> {
            if (rootContainer == null) return;

            long now = android.os.SystemClock.uptimeMillis();
            android.view.MotionEvent event = android.view.MotionEvent.obtain(
                now, now, action, x, y, 0
            );
            rootContainer.dispatchTouchEvent(event);
            event.recycle();
        });
    }

    private static void logViewHierarchy(View view, int depth) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < depth; i++) sb.append("  ");
        sb.append(view.getClass().getSimpleName());
        sb.append(" [").append(view.getWidth()).append("x").append(view.getHeight()).append("]");
        sb.append(" vis=").append(view.getVisibility() == View.VISIBLE ? "V" : view.getVisibility() == View.GONE ? "G" : "I");
        if (view instanceof android.view.SurfaceView) {
            sb.append(" (SurfaceView)");
        }
        Log.i(TAG, sb.toString());
        if (view instanceof ViewGroup) {
            ViewGroup vg = (ViewGroup) view;
            for (int i = 0; i < vg.getChildCount(); i++) {
                logViewHierarchy(vg.getChildAt(i), depth + 1);
            }
        }
    }
}
