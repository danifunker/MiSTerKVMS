import SwiftUI

/// MiSTerKVMS ▸ Settings: what's not per host, and (the DMG app) the
/// capture server in the background.
struct SettingsView: View {
    @ObservedObject var controller: SessionController
    @State private var viewerName = ""

    var body: some View {
        Form {
            Section {
                TextField("Viewer name", text: $viewerName)
                    .onSubmit { controller.viewerName = viewerName }
                Text("Other people watching a MiSTerKVMS Capture stream see this name, for example when you have control.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Toggle("Capture keyboard and mouse automatically", isOn: $controller.autoCapture)
                Text("When a host connects and when you click the picture. Off, you can just watch: Capture (⇧⌘K) sends your keyboard and mouse to the MiSTer. Starting the app with --auto-capture turns it on for that run.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Toggle("Show stats when the connection changes", isOn: $controller.statsOnChange)
                Text("Flashes the stats overlay for a few seconds when a session connects, drops or changes resolution. They're always one ⌘I away, or pinned with ⇧⌘I.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Toggle("Put the core and time under screenshots", isOn: $controller.screenshotStamp)
                Text("Copy Screenshot (⇧⌘C) and Save Screenshot (⇧⌘S) add a bar under the picture saying which core was running and when. Saved screenshots go to Pictures/MiSTerKVMS.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            #if !APP_STORE
            CaptureServerSettings()
            #endif
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .onAppear { viewerName = controller.viewerName }
        .onDisappear { controller.viewerName = viewerName }
    }
}
