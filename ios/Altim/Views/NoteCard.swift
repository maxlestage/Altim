import SwiftUI
import AltimKit

/// Personal notes on an asset (why I bought, my plan, when I sell): kept in the protected local store of the iPhone,
/// like the holdings (excluded from backups, never sent to the server).
struct NoteCard: View {
    let asset: Asset
    @State private var text = ""
    @State private var updated: Date?
    @FocusState private var focused: Bool

    private struct Note: Codable { var text: String; var updated: Date }

    var body: some View {
        Card(title: "Mes notes · \(asset.symbol)") {
            TextField("Pourquoi j'achète, mon plan, quand je vends…", text: $text, axis: .vertical)
                .lineLimit(3...8)
                .focused($focused)
                .padding(10)
                .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10))
                .accessibilityLabel("Mes notes sur \(asset.symbol)")
            Text((updated.map { "Enregistré le \(Format.date($0.timeIntervalSince1970 * 1000, time: true)). " } ?? "")
                 + "Gardé sur cet iPhone seulement. Relire sa thèse avant d'acheter ou de vendre évite les décisions sur un coup de tête.")
                .font(.caption).foregroundStyle(Theme.textSecondary)
        }
        .onAppear {
            let note = (LocalStore.load([String: Note].self, "notes") ?? [:])[asset.id]
            text = note?.text ?? ""
            updated = note?.updated
        }
        .onChange(of: focused) { _, isFocused in if !isFocused { save() } }
        .onDisappear { save() }
    }

    private func save() {
        var all = LocalStore.load([String: Note].self, "notes") ?? [:]
        let clean = String(text.trimmingCharacters(in: .whitespacesAndNewlines).prefix(2000))
        if clean == (all[asset.id]?.text ?? "") { return }
        if clean.isEmpty { all[asset.id] = nil } else { all[asset.id] = Note(text: clean, updated: Date()) }
        LocalStore.save(all, "notes")
        updated = all[asset.id]?.updated
    }
}
