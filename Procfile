web: ALTIM_WEB_ROOT=/app/web ./backend/target/release/altim
release: test -f web/dist/index.html && test -f web/dist/app/index.html && ls web/dist/*.wasm >/dev/null 2>&1 || { echo "Build du site manquant (web/dist) : déploiement refusé"; exit 1; }; test -x backend/target/release/altim || { echo "Serveur non compilé (backend/target/release/altim) : déploiement refusé"; exit 1; }
