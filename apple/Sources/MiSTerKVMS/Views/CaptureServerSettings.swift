#if !APP_STORE
import AppKit
import SwiftUI

/// Settings ▸ Capture Server (the DMG app only): run the capture server that
/// ships inside the app in the background, so this Mac shares its capture
/// cards. It's a launch agent for this user (`misterkvms-capture service
/// install`, server/src/service.rs): it starts at login, restarts if it
/// stops, and updates with the app.
struct CaptureServerSettings: View {
    @State private var status: ServerStatus?
    @State private var busy = false
    @State private var error: String?

    var body: some View {
        Section {
            LabeledContent("Status") {
                if busy {
                    ProgressView().controlSize(.small)
                } else {
                    Label(statusText, systemImage: statusIcon)
                        .foregroundStyle(status?.state == "running" ? .green : .secondary)
                }
            }
            HStack {
                if status?.installed == true {
                    if status?.state == "running" {
                        Button("Stop") { run("stop") }
                    } else {
                        Button("Start") { run("start") }
                    }
                    Button("Remove") { run("uninstall") }
                } else {
                    Button("Run in the background") { run("install") }
                        .buttonStyle(.borderedProminent)
                }
                Spacer()
                Button("Open setup page") { NSWorkspace.shared.open(URL(string: "http://localhost:5971")!) }
                    .disabled(status?.state != "running")
            }
            .disabled(busy || Self.program == nil)
            if let error {
                Text(error).font(.caption).foregroundStyle(.red)
            }
        } header: {
            Text("Capture Server")
        } footer: {
            Text("Shares the capture cards plugged into this Mac with MiSTerKVMS on your other devices. It starts when you log in and updates with this app; add streams on its setup page. The first time a stream starts, macOS asks to let “MiSTerKVMS Capture” use the camera and microphone (the card's picture and sound).")
                .font(.caption).foregroundStyle(.secondary)
        }
        .task { await refresh() }
    }

    private var statusText: String {
        guard let status else { return Self.program == nil ? "Not included in this build" : "Checking…" }
        if !status.installed { return "Not running in the background" }
        return status.state == "running" ? "Running: starts when you log in" : "Stopped"
    }

    private var statusIcon: String {
        status?.state == "running" ? "checkmark.circle.fill" : "pause.circle"
    }

    /// The server inside the app bundle.
    private static var program: URL? { Bundle.main.url(forAuxiliaryExecutable: "misterkvms-capture") }

    private func run(_ action: String) {
        busy = true
        error = nil
        Task {
            let result = await Self.service(action)
            if case let .failure(message) = result { error = message }
            // launchd takes a moment to start or stop it.
            try? await Task.sleep(for: .seconds(1))
            await refresh()
            busy = false
        }
    }

    private func refresh() async {
        if case let .success(out) = await Self.service("status") {
            status = try? JSONDecoder().decode(ServerStatus.self, from: Data(out.utf8))
        }
    }

    private enum Result { case success(String), failure(String) }

    /// `misterkvms-capture service <action>`, off the main thread.
    private static func service(_ action: String) async -> Result {
        guard let program else { return .failure("The capture server isn't in this build.") }
        return await Task.detached {
            let process = Process()
            process.executableURL = program
            process.arguments = ["service", action]
            let out = Pipe(), err = Pipe()
            process.standardOutput = out
            process.standardError = err
            do { try process.run() } catch { return .failure(error.localizedDescription) }
            let output = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            let errors = String(decoding: err.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            process.waitUntilExit()
            if process.terminationStatus == 0 { return .success(output) }
            let message = errors.trimmingCharacters(in: .whitespacesAndNewlines)
            return .failure(message.hasPrefix("error: ") ? String(message.dropFirst(7)) : message)
        }.value
    }
}

/// `misterkvms-capture service status` (server/src/service.rs `Status`).
private struct ServerStatus: Decodable {
    let installed: Bool
    let state: String
    let exe: String
}
#endif
