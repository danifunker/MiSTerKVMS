import SwiftUI

/// MiSTerKVMS ▸ Settings: what's not per host, and (the DMG app) the
/// capture server in the background.
struct SettingsView: View {
    @ObservedObject var controller: SessionController
    @State private var viewerName = ""
    #if APP_STORE
    @ObservedObject private var saveFolders = SaveFolders.shared
    #endif

    var body: some View {
        Form {
            Section {
                TextField("Viewer name", text: $viewerName)
                    .onSubmit { controller.viewerName = viewerName }
                Text("Other people watching a MiSTerKVMS Capture stream see this name, for example when you have control.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Toggle("Capture keyboard and mouse when a host connects", isOn: $controller.autoCapture)
                Text("Off, a host connects for watching: click the picture or press Capture (⇧⌘K) to send your keyboard and mouse to the MiSTer. Starting the app with --auto-capture turns it on for that run.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Picker("Release keyboard and mouse with", selection: $controller.releaseChord) {
                    ForEach(controller.releaseChordOptions(), id: \.id) { option in
                        Text(option.name).tag(option.chord)
                    }
                }
                Text("Press both together, then let go. \(controller.releaseChordOptions().first { $0.chord == controller.releaseChord }?.note ?? "")")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Toggle("Show stats when the connection changes", isOn: $controller.statsOnChange)
                Text("Flashes the stats overlay for a few seconds when a session connects, drops or changes resolution. They're always one ⌘I away, or pinned with ⇧⌘I.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Toggle("Put the core and time under screenshots", isOn: $controller.screenshotStamp)
                Text("Copy Screenshot (⇧⌘C) and Save Screenshot (⇧⌘S) add a bar under the picture saying which core was running and when.\(Self.screenshotsGo)")
                    .font(.caption).foregroundStyle(.secondary)
            }
            #if APP_STORE
            Section("Saving") {
                ForEach(SaveFolders.Kind.allCases, id: \.self) { kind in
                    LabeledContent(kind.title) {
                        HStack {
                            if saveFolders.missing.contains(kind) {
                                Text("Folder not found, asks").foregroundStyle(.secondary)
                            } else {
                                Text(saveFolders.displayName(kind) ?? "Ask each time")
                                    .foregroundStyle(.secondary)
                            }
                            Button("Choose…") { saveFolders.choose(kind) }
                            if saveFolders.folders[kind] != nil || saveFolders.missing.contains(kind) {
                                Button("Ask Each Time") { saveFolders.reset(kind) }
                            }
                        }
                    }
                }
                Text("With a folder chosen, Save Screenshot (⇧⌘S) and Record (⇧⌘R) save there straight away. Without one, they ask where, and can remember the folder you pick.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            #endif
            #if !APP_STORE
            CaptureServerSettings()
            #endif
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .onAppear { viewerName = controller.viewerName }
        .onDisappear { controller.viewerName = viewerName }
    }

    #if APP_STORE
    private static let screenshotsGo = " Where they're saved is under Saving, below."
    #else
    private static let screenshotsGo = " Saved screenshots go to Pictures/MiSTerKVMS."
    #endif
}
