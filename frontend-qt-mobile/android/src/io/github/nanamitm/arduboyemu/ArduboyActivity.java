package io.github.nanamitm.arduboyemu;

import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;
import android.os.Bundle;

import org.qtproject.qt.android.bindings.QtActivity;

/**
 * Retains arduboy:// VIEW intents long enough for the native Qt event loop to
 * collect them. QtActivity otherwise consumes a new intent before QML has a
 * public callback for it.
 */
public class ArduboyActivity extends QtActivity {
    private static final String PREFERENCES = "rom-transfer";
    private static final String PENDING_URL = "pending-url";
    private static ArduboyActivity instance;

    @Override
    public void onCreate(Bundle savedInstanceState) {
        instance = this;
        storeRomUrl(getIntent());
        super.onCreate(savedInstanceState);
    }

    @Override
    public void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        storeRomUrl(intent);
    }

    private void storeRomUrl(Intent intent) {
        if (intent == null || !Intent.ACTION_VIEW.equals(intent.getAction()))
            return;
        Uri data = intent.getData();
        if (data == null || !"arduboy".equalsIgnoreCase(data.getScheme()))
            return;
        getSharedPreferences(PREFERENCES, MODE_PRIVATE).edit()
            .putString(PENDING_URL, data.toString())
            .apply();
    }

    /** Called from Qt through JNI; returns each received URI once. */
    public static String takePendingRomUrl() {
        if (instance == null)
            return "";
        SharedPreferences preferences = instance.getSharedPreferences(PREFERENCES, MODE_PRIVATE);
        String value = preferences.getString(PENDING_URL, "");
        preferences.edit().remove(PENDING_URL).apply();
        return value;
    }
}
