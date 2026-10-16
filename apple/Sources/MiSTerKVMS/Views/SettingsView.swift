import SwiftUI

/// MiSTerKVMS ▸ Settings: what's not per host, and (the DMG app) the
/// capture server in the background.
struct SettingsView: View {
    let controller: SessionController
    @State private var viewerName = ""

    var body: some View {
        Form {
            Section {
                TextField("Viewer name", text: $viewerName)
                    .onSubmit { controller.viewerName = viewerName }
                Text("Other people watching a MiSTerKVMS Capture stream see this name, for example when you have control.")
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
