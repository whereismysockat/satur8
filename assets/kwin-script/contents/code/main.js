// Satur8 focus forwarder.
//
// KWin does not expose the active window to third-party processes, so this tiny
// script (running inside KWin, which does see it) forwards each window
// activation to the satur8 daemon over D-Bus. It changes nothing about the
// window itself - it only reports "this class is now focused" so the daemon can
// apply or restore the matching per-game profile.
//
// The window's output name is forwarded too, so the daemon can limit the color change to the display the game is actually on.
//
// Zero polling: we react only to KWin's own windowActivated signal.

function reportActive(window) {
    if (!window) {
        // Focus left to the desktop/null - report an empty class so the daemon
        // restores the default.
        callDBus("org.satur8.Daemon", "/org/satur8/Daemon",
                 "org.satur8.Daemon", "WindowActivatedWithPid", "", "", "", 0);
        return;
    }
    var cls = window.resourceClass ? "" + window.resourceClass : "";
    var cap = window.caption ? "" + window.caption : "";
    var output = window.output && window.output.name ? "" + window.output.name : "";
    // Proton games can share a generic class (steam_app_default). The owning
    // pid lets the daemon resolve the real exe and match the right profile.
    var pid = window.pid ? window.pid : 0;
    callDBus("org.satur8.Daemon", "/org/satur8/Daemon",
             "org.satur8.Daemon", "WindowActivatedWithPid", cls, cap, output, pid);
}

// KWin 6 scripting API: workspace.windowActivated(window).
if (workspace.windowActivated) {
    workspace.windowActivated.connect(reportActive);
} else if (workspace.clientActivated) {
    // Fallback for older API naming.
    workspace.clientActivated.connect(reportActive);
}

// Report whatever is focused right now, so applying state doesn't wait for the
// next focus change.
reportActive(workspace.activeWindow || (workspace.activeClient || null));
