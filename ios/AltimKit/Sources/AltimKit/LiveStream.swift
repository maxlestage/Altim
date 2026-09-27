import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Receives the Server-Sent Events of /api/live as they arrive (serial delegate queue: the parser is never shared).
final class LiveDelegate: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    private var parser = SSEParser()
    private let continuation: AsyncThrowingStream<LiveTick, Error>.Continuation

    init(_ continuation: AsyncThrowingStream<LiveTick, Error>.Continuation) { self.continuation = continuation }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
                    completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        if status == 200 { return completionHandler(.allow) }
        continuation.finish(throwing: status == 401 ? AltimError.unauthorized : AltimError.server("Flux en direct refusé (code \(status))."))
        completionHandler(.cancel)
    }

    /// A redirect is refused (the session cookie must stay on the server's host): the 30x ends the stream.
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        for tick in parser.ticks(data) { continuation.yield(tick) }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error { continuation.finish(throwing: AltimError.network(error.localizedDescription)) } else { continuation.finish() }
    }
}

extension AltimClient {
    /// Live prices of these assets: last known price right away, then every change (several per second for
    /// cryptos, every 5 s for stocks). Ends on a network error or an expired session (`.unauthorized`);
    /// the caller reconnects. Cancelling the consuming task closes the connection.
    public func liveTicks(_ assets: [Asset]) -> AsyncThrowingStream<LiveTick, Error> {
        AsyncThrowingStream { continuation in
            let queue = OperationQueue()
            queue.maxConcurrentOperationCount = 1
            let config = URLSessionConfiguration.default
            config.httpShouldSetCookies = false
            config.httpCookieAcceptPolicy = .never
            config.urlCache = nil
            config.timeoutIntervalForRequest = 60 // heartbeat every 15 s from the server
            config.timeoutIntervalForResource = 24 * 3600
            let stream = URLSession(configuration: config, delegate: LiveDelegate(continuation), delegateQueue: queue)
            let task = stream.dataTask(with: liveRequest(assets))
            continuation.onTermination = { _ in
                task.cancel()
                stream.invalidateAndCancel()
            }
            task.resume()
        }
    }
}
