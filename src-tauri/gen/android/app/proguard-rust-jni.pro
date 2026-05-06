-keep class com.cla.verger.android.MainActivity {
    public static boolean openYamlFileFromRust();
    public static boolean saveYamlFileFromRust(java.lang.String);
    public static boolean requestVpnFromRust();
    public static boolean stopVpnFromRust();
}

-keep class com.cla.verger.android.ClashVpnService {
    public static boolean isRunning();
    public static boolean isTunnelRunning();
}
