import SwiftUI
import AltimKit

/// Agenda (Actu → Agenda): the coming days' economic releases, central bank decisions, earnings, dividends, splits and
/// IPOs, each with its source; what no free source covers is listed at the bottom. Same content as the web
/// (web/src/webapp/Agenda.tsx). Chips wrap, events are stacked cards: nothing scrolls sideways.
struct AgendaView: View {
    @Environment(AppModel.self) private var model
    @Binding var view: String
    @State private var report: CalendarReport?
    @State private var error: String?
    @State private var loading = false
    @State private var days = 14
    @AppStorage("agenda.filter") private var filterRaw = Agenda.Filter.all.rawValue
    @AppStorage("agenda.mine") private var mine = false

    private var filter: Agenda.Filter { Agenda.Filter(rawValue: filterRaw) ?? .all }

    /// Stock symbols of the radar and the holdings (company events exist for stocks only).
    private var stocks: [String] { Agenda.stockSymbols(model.watchlist + model.holdings.map(\.asset)) }

    /// "Mes actifs": the server gives these stocks' events (even small companies); without any stock, the whole
    /// calendar is filtered on the iPhone.
    private var asked: [String]? { mine && !stocks.isEmpty ? stocks : nil }

    var body: some View {
        let shown = report.map { Agenda.filter($0.events, filter, mine: mine ? stocks : nil) } ?? []
        let groups = Agenda.groupByDay(shown)
        let failed = report?.sources.filter { !$0.ok } ?? []
        List {
            Section { NewsViewPicker(view: $view) }.listRowBackground(Color.clear)
            Section {
                RiskWeekCard(held: Agenda.stockSymbols(model.holdings.map(\.asset)), watched: Agenda.stockSymbols(model.watchlist))
            }
            .listRowBackground(Color.clear)
            Section { controls }.listRowBackground(Color.clear)
            if let error { Section { Notice(text: error, tone: .warn) }.listRowBackground(Color.clear) }
            if !failed.isEmpty {
                Section { Notice(text: failedText(failed), tone: .warn) }.listRowBackground(Color.clear)
            }
            if report != nil && loading {
                Section { Text("Mise à jour…").font(.caption).foregroundStyle(Theme.textSecondary) }.listRowBackground(Color.clear)
            }
            if report != nil && groups.isEmpty {
                Section {
                    Text("Aucun événement de ce type sur la période.").font(.footnote).foregroundStyle(Theme.textSecondary)
                }
                .listRowBackground(Color.clear)
            }
            ForEach(groups) { g in
                Section {
                    ForEach(Array(g.events.enumerated()), id: \.offset) { item in
                        EventRow(event: item.element)
                    }
                    .listRowBackground(Theme.surface.opacity(0.6))
                } header: {
                    Text(Agenda.dayLabel(g.day))
                }
            }
            if let report { notCovered(report) }
        }
        .listStyle(.insetGrouped)
        .overlay { if report == nil && error == nil { ProgressView("Chargement de l'agenda…") } }
        .refreshable { await load() }
        .task(id: "\(days)|\(asked?.joined(separator: ",") ?? "")") {
            // Refreshed every 15 minutes while the agenda is on screen.
            while !Task.isCancelled {
                await load()
                try? await Task.sleep(for: .seconds(900))
            }
        }
    }

    private var controls: some View {
        Card(title: "Agenda") {
            Text("Publications économiques majeures, décisions des banques centrales, résultats, dividendes, splits et introductions en bourse. Heures de Paris ; chiffres tels que publiés par la source.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            FittingPicker(title: "Période", selection: $days) {
                ForEach(Agenda.periods, id: \.self) { Text("\($0) jours").tag($0) }
            }
            WrapLayout(spacing: 6) {
                ForEach(Agenda.Filter.allCases, id: \.self) { f in
                    Button { filterRaw = f.rawValue } label: { TagChip(text: f.label, color: .white, selected: filter == f) }
                        .buttonStyle(.borderless)
                        .accessibilityAddTraits(filter == f ? .isSelected : [])
                }
                Button { mine.toggle() } label: { TagChip(text: "\(mine ? "✓ " : "")Mes actifs", color: .white, selected: mine) }
                    .buttonStyle(.borderless)
                    .accessibilityAddTraits(mine ? .isSelected : [])
            }
            if mine {
                Text(stocks.isEmpty
                     ? "Aucune action dans votre radar ni vos avoirs : seules l'économie et les banques centrales, qui concernent aussi les cryptos, sont affichées."
                     : "Résultats, dividendes et splits de vos \(stocks.count) action\(stocks.count > 1 ? "s" : "") (radar et avoirs), plus l'économie et les banques centrales, qui concernent tous les actifs, cryptos compris.")
                    .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func failedText(_ failed: [CalendarReport.Source]) -> String {
        let list = failed.map { s -> String in
            guard let n = s.failed?.count, n > 0 else { return s.name }
            return "\(s.name) (\(n) jour\(n > 1 ? "s" : "") manquant\(n > 1 ? "s" : ""))"
        }
        return "Sources incomplètes : \(list.joined(separator: ", "))."
    }

    @ViewBuilder private func notCovered(_ r: CalendarReport) -> some View {
        Section {
            ForEach(r.notCovered, id: \.self) { t in
                Text("• \(t)").font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            }
            Text("« · » sépare plusieurs séries publiées sous le même nom par la source (souvent la variation sur un mois et sur un an), dans l'ordre de la source.")
                .font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
            DisclosureGroup("Sources · \(r.sources.filter(\.ok).count)/\(r.sources.count) en ligne") {
                ForEach(r.sources) { s in
                    Text(s.ok ? "✔ \(s.name)" : "✕ \(s.name) · \(s.error ?? "indisponible")\(s.failed.map { $0.isEmpty ? "" : " (\($0.joined(separator: ", ")))" } ?? "")")
                        .font(.caption).foregroundStyle(s.ok ? .white : Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                }
            }
            .font(.footnote)
        } header: {
            Text("Non couvert")
        }
        .listRowBackground(Theme.surface.opacity(0.6))
    }

    private func load() async {
        guard let client = model.client else { return }
        loading = true
        defer { loading = false }
        do {
            report = try await client.calendar(days: days, symbols: asked)
            error = nil
            model.persistSession()
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// Risk of the next 7 days, one stacked row per day (weekends included): its own request, with the user's stocks and
/// the largest companies together (`top=1`), whatever the list's filters. Same rule and texts as the web (Agenda.tsx
/// `RiskWeek`); the level is written out and carried by its emoji, never by the colour alone.
struct RiskWeekCard: View {
    @Environment(AppModel.self) private var model
    let held: [String]
    let watched: [String]
    @State private var report: CalendarReport?
    @State private var error: String?

    private var stocks: [String] {
        var seen = Set<String>()
        return Array((held + watched).filter { seen.insert($0).inserted }.prefix(50))
    }

    var body: some View {
        let days = report.map { Agenda.riskDays($0.events, from: $0.from, held: held, watched: watched, n: 7, failed: Agenda.failedDays($0)) } ?? []
        Card(title: "Calendrier de risque · 7 jours") {
            if let error { Notice(text: error, tone: .warn) }
            if report == nil && error == nil {
                Text("Chargement…").font(.caption).foregroundStyle(Theme.textSecondary)
            }
            ForEach(days) { d in dayRow(d) }
            DisclosureGroup("Règle") {
                Text(Agenda.riskRule).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 4)
            }
            .font(.footnote)
            .tint(.white)
        }
        .task(id: stocks.joined(separator: ",")) { await load() }
    }

    private func color(_ d: Agenda.RiskDay) -> Color {
        if d.unknown { return Theme.textSecondary }
        switch d.level {
        case .high: return Theme.sell
        case .medium: return Theme.warning
        case .low: return Theme.buy
        }
    }

    private func dayRow(_ d: Agenda.RiskDay) -> some View {
        let label = d.unknown ? "Risque non évalué" : d.level.label
        return VStack(alignment: .leading, spacing: 3) {
            WrapLayout(spacing: 6) {
                Text(d.label).font(.footnote.weight(.semibold)).foregroundStyle(.white)
                if d.weekend { TagChip(text: "week-end") }
                Text(d.unknown ? "⚪" : d.level.icon).font(.footnote).accessibilityLabel(label)
            }
            Text(Agenda.riskText(d)).font(.caption).foregroundStyle(.white.opacity(0.9)).fixedSize(horizontal: false, vertical: true)
            if d.incomplete && !d.unknown {
                Text("Sources incomplètes ce jour-là.").font(.caption2).foregroundStyle(Theme.textSecondary)
            }
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(color(d).opacity(0.08)))
        .overlay(alignment: .leading) { Rectangle().fill(color(d).opacity(0.7)).frame(width: 3) }
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .accessibilityElement(children: .combine)
    }

    private func load() async {
        guard let client = model.client else { return }
        do {
            report = try await client.calendar(days: 7, symbols: stocks.isEmpty ? nil : stocks, top: true)
            error = nil
        } catch AltimError.unauthorized {
            model.sessionLost()
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// One event: importance (dot + words for VoiceOver), time, title, tags, figures, detail, note and its source.
struct EventRow: View {
    let event: CalendarEvent
    /// Replaces the time (the decision card writes the day before it: "Demain · 14:30").
    var timeLabel: String? = nil
    @Environment(\.openURL) private var openURL

    var body: some View {
        let e = event
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Circle().fill(e.high ? Theme.sell : Theme.warning).frame(width: 8, height: 8)
                    .accessibilityLabel(e.high ? "Importance haute" : "Importance moyenne")
                Text(timeLabel ?? e.time ?? "Journée").font(Theme.mono(12, weight: .regular)).foregroundStyle(Theme.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(e.title).font(.footnote.weight(.semibold)).foregroundStyle(.white).fixedSize(horizontal: false, vertical: true)
            }
            WrapLayout(spacing: 4) {
                TagChip(text: e.categoryLabel)
                if let c = e.country { TagChip(text: c) }
                if let s = e.symbol { TagChip(text: s, color: .white) }
                if let o = e.originalName, o != e.title { TagChip(text: o).accessibilityLabel("Nom publié par la source : \(o)") }
            }
            figures
            if let d = e.detail { Text(d).font(.caption).foregroundStyle(Theme.textSecondary).fixedSize(horizontal: false, vertical: true) }
            if let n = e.note { Text(n).font(.caption).foregroundStyle(Theme.warning).fixedSize(horizontal: false, vertical: true) }
            Button {
                if let url = e.link { openURL(url) }
            } label: {
                Text("Source : \(e.source)").font(.caption).foregroundStyle(Theme.cyan).multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .buttonStyle(.borderless)
            .accessibilityHint("Ouvre la page de la source")
        }
        .padding(.vertical, 4)
        .overlay(alignment: .leading) {
            if e.high { Rectangle().fill(Theme.sell.opacity(0.5)).frame(width: 2).offset(x: -10) }
        }
    }

    @ViewBuilder private var figures: some View {
        let e = event
        if e.actual != nil || e.consensus != nil || e.previous != nil {
            let earnings = e.kind == .earnings
            WrapLayout(spacing: 12) {
                if let a = e.actual { figure("Publié", a) }
                if let c = e.consensus { figure(earnings ? "Consensus" : "Attendu", c) }
                if let p = e.previous {
                    if earnings {
                        Text(p).font(.caption).foregroundStyle(Theme.textSecondary)
                    } else {
                        figure("Précédent", p)
                    }
                }
            }
        }
    }

    private func figure(_ label: String, _ value: String) -> some View {
        (Text("\(label) ").foregroundStyle(Theme.textSecondary) + Text(value).bold().foregroundStyle(.white)).font(.caption)
    }
}
