import Combine
#if canImport(KVMSCore)
import KVMSCore
#endif
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var profiles: ProfileStore
    @EnvironmentObject private var controller: SessionController
    @State private var selection: HostProfile.ID?
    @State private var editing: HostProfile?

    var body: some View {
        NavigationSplitView {
            GeometryReader { geo in
            List(selection: $selection) {
                ForEach(profiles.profiles) { profile in
                    HostRow(profile: profile, isLive: controller.profile?.id == profile.id,
                            preview: controller.previews[profile.id],
                            pictureHeight: Self.pictureHeight(for: geo.size, hosts: profiles.profiles.count))
                        .tag(profile.id)
                        .contextMenu {
                            if !profile.isDemo {
                                Button("Edit…") { editing = profile }
                            }
                            Button("Delete", role: .destructive) { delete(profile.id) }
                        }
                }
            }
            }
            .navigationSplitViewColumnWidth(min: 200, ideal: 240)
            .toolbar {
                ToolbarItem {
                    Menu {
                        Button("Add Host…") { editing = HostProfile.make() }
                        Button("Add Demo Host") { addDemo() }
                    } label: {
                        Label("Add Host", systemImage: "plus")
                    } primaryAction: {
                        editing = HostProfile.make()
                    }
                    .help("Add a host (hold for the demo host)")
                }
            }
        } detail: {
            // The banner belongs to the detail column; on the whole split view
            // it would draw over the sidebar.
            VStack(spacing: 0) {
                #if !APP_STORE
                AccessibilityBanner()
                #endif
                if let profile = controller.profile {
                    SessionView(profile: profile, onEdit: { editing = profile })
                } else if profiles.profiles.isEmpty {
                    // No hosts yet: what to do goes here, not in the empty sidebar.
                    ContentUnavailableView {
                        Label("No Hosts", systemImage: "display.2")
                    } description: {
                        Text("Add a MiSTer: a stream from MiSTerKVMS Capture, or a capture card on this Mac.")
                    } actions: {
                        Button("Add Host…") { editing = HostProfile.make() }
                        Button("Try Without a MiSTer") { addDemo() }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ContentUnavailableView("Select a Host", systemImage: "sidebar.left",
                                           description: Text("Choose a MiSTer in the sidebar to connect."))
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .sheet(item: $editing) { profile in
            ProfileEditor(profile: profile) { saved in
                controller.save(saved)
            }
        }
        // Disconnected (toolbar, Session menu): nothing selected, so clicking
        // the host connects again.
        .onChange(of: controller.profile?.id) { _, id in
            if id == nil { selection = nil }
        }
        // Closing the window ends the session (sound, input); the app stays.
        .onDisappear { controller.close() }
        .onChange(of: selection) { _, id in
            if let id, let profile = profiles.profiles.first(where: { $0.id == id }) {
                controller.open(profile)
            } else {
                controller.close()
            }
        }
    }

    /// How tall the hosts' pictures are: as big as the sidebar's width
    /// allows (4:3) while one or two hosts fit, smaller as more are added so
    /// they still fit, down to a size that's still recognizable.
    static func pictureHeight(for size: CGSize, hosts: Int) -> CGFloat {
        let widest = max(size.width - 32, 60) * 3 / 4
        let text: CGFloat = 52
        let fit = (size.height - 16) / CGFloat(max(hosts, 1)) - text
        return min(max(fit, 54), widest, 220)
    }

    /// The demo host (core: demo_host_profile): the test pattern, input
    /// listed on the picture. For trying the app, and App Review.
    private func addDemo() {
        let demo = demoHostProfile()
        if !profiles.profiles.contains(where: { $0.id == demo.id }) {
            profiles.upsert(demo)
            profiles.reload()
        }
        selection = demo.id
    }

    private func delete(_ id: HostProfile.ID) {
        if controller.profile?.id == id { controller.close() }
        if selection == id { selection = nil }
        profiles.delete(id)
    }
}

/// A host in the sidebar: its picture (kept fresh by the core), the name,
/// and what it's running.
private struct HostRow: View {
    let profile: HostProfile
    let isLive: Bool
    let preview: SessionController.Preview?
    let pictureHeight: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            picture
            HStack {
                Text(profile.name).fontWeight(isLive ? .semibold : .regular)
                if isLive {
                    Image(systemName: "dot.radiowaves.left.and.right").foregroundStyle(.green)
                }
            }
            .lineLimit(1)
            Text(caption)
                .font(.caption).foregroundStyle(.secondary).lineLimit(1)
        }
        .padding(.vertical, 4)
    }

    private var picture: some View {
        ZStack {
            Color.black
            if let image = preview?.image {
                Image(nsImage: image).resizable().interpolation(.medium)
            } else {
                Image(systemName: profile.isDemo ? "play.tv" : "display")
                    .font(.system(size: min(pictureHeight / 3, 28)))
                    .foregroundStyle(.secondary)
            }
        }
        .aspectRatio(preview?.image == nil ? 4.0 / 3.0 : preview?.aspect ?? 4.0 / 3.0, contentMode: .fit)
        .frame(maxWidth: .infinity, maxHeight: pictureHeight)
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.white.opacity(0.1)))
    }

    /// The core running, or where the host's picture comes from until
    /// that's known.
    private var caption: String {
        if let caption = preview?.caption, !caption.isEmpty { return caption }
        return profile.subtitle
    }
}

/// Explains why Accessibility is needed (D5) until it's granted.
private struct AccessibilityBanner: View {
    @State private var trusted = InputCapture.hasAccessibility
    private let timer = Timer.publish(every: 2, on: .main, in: .common).autoconnect()

    var body: some View {
        Group {
            if !trusted {
                HStack(spacing: 12) {
                    Image(systemName: "keyboard.badge.exclamationmark").font(.title2)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Allow Accessibility to send every key to MiSTer").fontWeight(.semibold)
                        Text("Without it, macOS keeps shortcuts like ⌘Tab, ⌘Space and ⌘Q for itself. MiSTerKVMS only intercepts input while you're in capture mode.")
                        Text("Already switched on? macOS ties the permission to the app's signature, so a newer or differently signed copy needs it again: remove MiSTerKVMS from the list with −, add it back, and reopen the app.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button("Open Settings…") { InputCapture.requestAccessibility() }
                }
                .padding(10)
                .background(.yellow.opacity(0.15))
            }
        }
        .onReceive(timer) { _ in trusted = InputCapture.hasAccessibility }
    }
}
