import SwiftUI
import AltimKit

/// « Pourquoi ça bouge ? » on the asset page (GET /api/why): observations that happen together, never presented as
/// causes, each with its direction, magnitude, source and certainty; the optional question box only when the server
/// offers it (`askEnabled`). Same texts as the web (WhyCard.tsx).
struct WhyCard: View {
    @Environment(AppModel.self) private var model
    let asset: Asset
    @State private var report: WhyReport?
    @State private var error: String?
    @State private var question = ""
    @State private var asking = false
    @State private var answer: AskAnswer?
    @State private var askError: String?

    var body: some View {
        Card(title: "Pourquoi ça bouge ?") {
            if let error { Notice(text: error, tone: .warn) }
            if report == nil && error == nil {
                ProgressView().frame(maxWidth: .infinity, minHeight: 60).accessibilityLabel("Chargement des observations")
            }
            if let r = report {
                Text(r.summary).font(.subheadline).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                ForEach(r.factors) { f in factorRow(f) }
                if let nc = r.notCoveredText {
                    Text(nc).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
                Text(r.disclaimer).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                if r.askEnabled == true { askBox }
            }
        }
        .task(id: asset.id) { await load() }
    }

    private func factorRow(_ f: WhyReport.Factor) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(f.direction.icon).font(.headline).foregroundStyle(Theme.color(f.certainty.tone)).accessibilityLabel(f.direction.word)
            VStack(alignment: .leading, spacing: 3) {
                WrapLayout(spacing: 6) {
                    Text(f.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                    TagChip(text: f.certaintyLabel)
                }
                Text(f.detail).font(.footnote).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
                Text(f.meta).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.color(f.certainty.tone).opacity(0.08)))
        .accessibilityElement(children: .combine)
    }

    private var askBox: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Poser une question sur ces données").font(.footnote.weight(.semibold)).foregroundStyle(.white)
            TextField("ex. La baisse vient-elle du marché ou de l'actif ?", text: $question, axis: .vertical)
                .lineLimit(2...5)
                .textFieldStyle(.roundedBorder)
                .onChange(of: question) { _, q in if q.count > Why.maxQuestion { question = String(q.prefix(Why.maxQuestion)) } }
            Button(asking ? "Réponse en cours…" : "Demander") { Task { await ask() } }
                .buttonStyle(NeonButtonStyle(filled: false))
                .disabled(asking || !Why.canAsk(question))
            Text(Why.askNote).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            if let askError { Notice(text: askError, tone: .warn) }
            if let answer {
                VStack(alignment: .leading, spacing: 4) {
                    Text(answer.answer).font(.footnote).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
                    Text(answer.dataText).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.05)))
                .accessibilityElement(children: .combine)
            }
        }
        .padding(.top, 4)
    }

    private func load() async {
        guard let client = model.client else { return }
        do {
            report = try await client.why(asset)
            error = nil
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            if report == nil { self.error = error.localizedDescription }
        }
    }

    private func ask() async {
        guard let client = model.client, Why.canAsk(question), !asking else { return }
        asking = true
        askError = nil
        answer = nil
        defer { asking = false }
        do {
            answer = try await client.ask(asset, question: question)
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch {
            askError = error.localizedDescription
        }
    }
}
