import SwiftUI

/// Settings for this iPhone (not one MiSTer): its name, demo mode, and
/// About (the version and credits, as in the desktop apps).
struct AppSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var demo = false
    @State private var stamp = true
    @State private var encryption = "auto"

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("My \(Device.kind)", text: $name)
                        .submitLabel(.done)
                } header: {
                    Text("This \(Device.kind)'s name")
                } footer: {
                    Text("Capture PCs list this \(Device.kind) by this name under their paired devices, and other viewers see it when you have control (“\(name.isEmpty ? Device.kind : name) has control”). A change shows on the capture PC the next time this \(Device.kind) connects.")
                }
                Section {
                    Picker("Encryption at home", selection: $encryption) {
                        Text("When the capture PC can").tag("auto")
                        Text("Always").tag("require")
                        Text("Off").tag("off")
                    }
                } footer: {
                    Text(encryption == "off"
                         ? "On your home network this \(Device.kind) connects without encryption (paired MiSTers still use their key). Away from home connections are always encrypted."
                         : "Connections to capture PCs on your home network are encrypted, and this \(Device.kind) remembers each capture PC's identity the first time, so another computer can't answer in its place. \(encryption == "require" ? "Capture PCs need MiSTerKVMS Capture 1.0.26 or later." : "Older capture PCs connect without encryption.")")
                }
                Section {
                    Toggle("Demo mode", isOn: $demo)
                } footer: {
                    Text("Adds a Demo host to the list: a test pattern with sound instead of a MiSTer, and everything you type or tap listed on screen. For trying the app without a MiSTer; your MiSTers aren't affected.")
                }
                Section {
                    Toggle("Core and time on screenshots", isOn: $stamp)
                } footer: {
                    Text("The screenshot button (a camera on a clipboard) copies the picture. With this on, a bar under it says which core was running and when.")
                }
                Section("About") {
                    LabeledContent("Version", value: version)
                    Credit(title: "MiSTer-Kun mascot") {
                        Text("Original design by HeWhoisRed, a gift to the MiSTer community. Remastered artwork by ")
                            + Text("[baxysquare/mister_kun](https://github.com/baxysquare/mister_kun)") + Text(".")
                    }
                    Credit(title: "MiSTer FPGA") {
                        Text("[MiSTer FPGA project](https://github.com/MiSTer-devel/Main_MiSTer/wiki) by Sorgelig and contributors.")
                    }
                    Credit(title: "Keyboard & mouse") {
                        Text("[mrext Remote](https://github.com/wizzomafizzo/mrext) by wizzo and contributors.")
                    }
                    Credit(title: "Built with") {
                        Text("Rust: wgpu, cpal, tungstenite, UniFFI. Swift: SwiftUI and UIKit.")
                    }
                    Link("Source on GitHub (MIT License)", destination: URL(string: "https://github.com/danifunker/MiSTerKVMS")!)
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        model.deviceName = name
                        if demo != model.demoMode { model.demoMode = demo }
                        if stamp != model.screenshotStamp { model.screenshotStamp = stamp }
                        if encryption != model.homeEncryption { model.homeEncryption = encryption }
                        dismiss()
                    }
                }
            }
            .onAppear {
                name = model.deviceNameIsDefault ? "" : model.deviceName
                demo = model.demoMode
                stamp = model.screenshotStamp
                encryption = model.homeEncryption
            }
        }
    }

    private var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? "dev"
        let build = info?["CFBundleVersion"] as? String ?? "0"
        return "\(short) (\(build))"
    }
}

private struct Credit<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.subheadline.weight(.semibold))
            content.font(.footnote).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 2)
    }
}
