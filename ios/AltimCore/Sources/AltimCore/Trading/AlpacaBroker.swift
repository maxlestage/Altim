import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Exécution d'actions US via Alpaca (paper trading ou compte réel).
public struct AlpacaBroker: Broker {
    public let name = "Alpaca"
    public let environment: BrokerEnvironment
    let keyId: String
    let secret: String
    let baseURL: URL
    let transport: HTTPTransport

    public init(keyId: String, secret: String, environment: BrokerEnvironment, transport: HTTPTransport = URLSessionTransport()) {
        self.keyId = keyId.trimmingCharacters(in: .whitespacesAndNewlines)
        self.secret = secret.trimmingCharacters(in: .whitespacesAndNewlines)
        self.environment = environment
        self.transport = transport
        self.baseURL = URL(string: environment == .live ? "https://api.alpaca.markets" : "https://paper-api.alpaca.markets")!
    }

    public func balances() async throws -> [Balance] {
        let account = try JSON.object(try await call("GET", "/v2/account"))
        var result: [Balance] = []
        if let cash = JSON.decimal(account["cash"]) {
            let buyingPower = JSON.decimal(account["buying_power"]) ?? cash
            result.append(Balance(asset: "USD", free: min(cash, buyingPower), locked: max(0, cash - buyingPower)))
        }
        let positions = try JSON.array(try await call("GET", "/v2/positions")) as? [[String: Any]] ?? []
        for p in positions {
            guard let symbol = p["symbol"] as? String, let qty = JSON.decimal(p["qty"]) else { continue }
            let available = JSON.decimal(p["qty_available"]) ?? qty
            result.append(Balance(asset: symbol, free: available, locked: qty - available))
        }
        return result
    }

    public func rules(for symbol: String) async throws -> SymbolRules {
        let encoded = symbol.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? symbol
        let asset = try JSON.object(try await call("GET", "/v2/assets/\(encoded)"))
        let tradable = (asset["tradable"] as? Bool) == true && (asset["status"] as? String) == "active"
        return SymbolRules(
            symbol: symbol, baseAsset: symbol, quoteAsset: "USD",
            // Les ordres « bracket » (avec stop et objectif) exigent des actions entières.
            stepSize: 1, minQuantity: 1, maxQuantity: 0,
            tickSize: JSON.decimal(asset["price_increment"]) ?? Decimal(string: "0.01")!,
            minNotional: 1, isTradable: tradable
        )
    }

    public func test(_ order: OrderRequest) async throws {
        // Alpaca n'a pas d'endpoint de test : validation locale + pouvoir d'achat.
        let rules = try await rules(for: order.symbol)
        let price: Decimal
        if let limit = order.limitPrice { price = limit } else { price = try await lastPrice(order.symbol) }
        var issues = rules.issues(for: order, referencePrice: price)
        if order.side == .buy {
            let account = try JSON.object(try await call("GET", "/v2/account"))
            if let bp = JSON.decimal(account["buying_power"]), order.quantity * price > bp {
                issues.append("Pouvoir d'achat insuffisant (\(bp.plainString) USD).")
            }
            if (account["trading_blocked"] as? Bool) == true { issues.append("Compte bloqué pour le trading.") }
        }
        if !issues.isEmpty { throw BrokerError.validation(issues) }
    }

    private func lastPrice(_ symbol: String) async throws -> Decimal {
        let encoded = symbol.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? symbol
        let url = URL(string: "https://data.alpaca.markets/v2/stocks/\(encoded)/trades/latest")!
        let obj = try JSON.object(try await call("GET", url: url))
        guard let price = JSON.decimal((obj["trade"] as? [String: Any])?["p"]) else { throw APIError.decoding("prix Alpaca") }
        return price
    }

    public func place(_ order: OrderRequest) async throws -> OrderResult {
        let rules = try await rules(for: order.symbol)
        var body: [String: Any] = [
            "symbol": order.symbol,
            "qty": order.quantity.plainString,
            "side": order.side == .buy ? "buy" : "sell",
            "type": order.type == .market ? "market" : "limit",
            "time_in_force": order.type == .market ? "day" : "gtc",
            "client_order_id": "altim-" + UUID().uuidString,
        ]
        if order.type == .limit, let price = order.limitPrice {
            body["limit_price"] = rules.normalizePrice(price).plainString
        }
        if order.side == .buy, let stop = order.stopLoss, let target = order.takeProfit {
            body["order_class"] = "bracket"
            body["take_profit"] = ["limit_price": rules.normalizePrice(target).plainString]
            body["stop_loss"] = ["stop_price": rules.normalizePrice(stop).plainString]
        }
        let obj = try JSON.object(try await call("POST", "/v2/orders", body: body))
        let executed = JSON.decimal(obj["filled_qty"]) ?? 0
        return OrderResult(
            orderId: obj["id"] as? String ?? "?", symbol: order.symbol, side: order.side,
            status: (obj["status"] as? String ?? "unknown").uppercased(), executedQuantity: executed,
            averagePrice: JSON.decimal(obj["filled_avg_price"]),
            protectionOrderId: body["order_class"] != nil ? (obj["id"] as? String) : nil, isSimulated: false,
            notes: body["order_class"] != nil ? ["Ordre bracket : stop et objectif attachés."] : []
        )
    }

    private func call(_ method: String, _ path: String, body: [String: Any]? = nil) async throws -> Data {
        try await call(method, url: baseURL.appendingPathComponent(path), body: body)
    }

    private func call(_ method: String, url: URL, body: [String: Any]? = nil) async throws -> Data {
        guard !keyId.isEmpty, !secret.isEmpty else { throw APIError.missingCredentials }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue(keyId, forHTTPHeaderField: "APCA-API-KEY-ID")
        request.setValue(secret, forHTTPHeaderField: "APCA-API-SECRET-KEY")
        if let body {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let (data, response) = try await transport.send(request)
        guard (200..<300).contains(response.statusCode) else {
            let obj = try? JSON.object(data)
            throw BrokerError.rejected(code: obj?["code"] as? Int ?? response.statusCode,
                                       message: obj?["message"] as? String ?? "Erreur \(response.statusCode)")
        }
        return data
    }
}
