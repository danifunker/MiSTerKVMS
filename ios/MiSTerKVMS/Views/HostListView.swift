import SwiftUI

/// The MiSTers (one per capture server stream). Tapping one opens it full
/// screen; one at a time (P13).
struct HostListView: View {
    @EnvironmentObject private var model: AppModel
    @State private var adding = false
    @State private var settingsFor: HostProfile?
    @State private var appSettings = false

    var body: some View {
        List {
            // The Demo host first, apart from the real MiSTers.
            ForEach(model.profiles.filter(\.isDemo)) { profile in
                Button {
                    model.open(profile)
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(profile.name).font(.headline)
                            Text(profile.subtitle).font(.subheadline).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text("DEMO")
                            .font(.caption2.weight(.bold))
                            .padding(.horizontal, 6).padding(.vertical, 2)
                            .background(Capsule().fill(.tint.opacity(0.25)))
                    }
                }
                .tint(.primary)
                .swipeActions {
                    Button { model.demoMode = false } label: { Label("Hide", systemImage: "eye.slash") }
                }
                .contextMenu {
                    Button { model.demoMode = false } label: { Label("Hide the demo", systemImage: "eye.slash") }
                }
            }
            ForEach(model.profiles.filter { !$0.isDemo }) { profile in
                Button {
                    model.open(profile)
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(profile.name).font(.headline)
                            Text(profile.subtitle).font(.subheadline).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if profile.useInternet {
                            Image(systemName: "globe").foregroundStyle(.secondary).accessibilityLabel("Also over the internet")
                        }
                    }
                }
                .tint(.primary)
                .swipeActions {
                    Button(role: .destructive) { model.delete(profile.id) } label: { Label("Delete", systemImage: "trash") }
                    Button { settingsFor = profile } label: { Label("Settings", systemImage: "gearshape") }
                }
                .contextMenu {
                    Button { settingsFor = profile } label: { Label("Settings", systemImage: "gearshape") }
                    Button(role: .destructive) { model.delete(profile.id) } label: { Label("Delete", systemImage: "trash") }
                }
            }
        }
        .overlay {
            if model.profiles.isEmpty {
                ContentUnavailableView {
                    Label("No MiSTers yet", systemImage: "display")
                } description: {
                    Text("You need a MiSTer FPGA running mrext Remote, a capture card showing it, and MiSTerKVMS Capture (Windows, Mac or Linux) on the computer the card is plugged into. Then add its stream here while you're on the same Wi-Fi.")
                } actions: {
                    Button("Add a MiSTer") { adding = true }.buttonStyle(.borderedProminent)
                    Button("Try it without a MiSTer") { model.demoMode = true }
                }
            }
        }
        .navigationTitle("MiSTerKVMS")
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button { appSettings = true } label: { Image(systemName: "gearshape") }
                    .accessibilityLabel("Settings")
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button { adding = true } label: { Image(systemName: "plus") }
            }
        }
        .onAppear { model.listShown() }
        .sheet(isPresented: $appSettings) { AppSettingsView() }
        .sheet(isPresented: $adding) { AddHostView() }
        .sheet(item: $settingsFor) { profile in HostSettingsView(hostId: profile.id) }
    }
}

extension HostProfile: Identifiable {}

extension HostProfile {
    /// The stream's name, from its address (`mkvm://pc/name`).
    var streamName: String {
        var path = Substring(stream)
        if let scheme = path.range(of: "://") { path = path[scheme.upperBound...] }
        guard let slash = path.firstIndex(of: "/") else { return "" }
        let name = path[path.index(after: slash)...].trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        return name.removingPercentEncoding ?? name
    }

    /// The demo host (core: `demo_host_profile`).
    var isDemo: Bool { source == "demo" }

    var subtitle: String {
        if isDemo { return "Test pattern; your input is listed on screen" }
        if !streamName.isEmpty { return "Stream \(streamName)" }
        if !misterHost.isEmpty { return misterHost }
        return "No stream chosen"
    }
}
