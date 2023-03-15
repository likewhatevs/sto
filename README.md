# sto

This repo contains a server, cli and UI to store/retrieve profiler data.

The cli stores/uploads profiler data to postgres, via the server. The cli profiles a provided locally running pid (via libbpf), symbolizes it (via blazesym) and posts it to the server.

The UI (templated html web page which GET's some json from the server) shows these stored profiles, along with the capacity savings the storage approach used by this repo uses.

The cli and server are buildable via cargo build --release.

The server and database are deployable via docker-compose (which is to be added after reading today's news about docker breaking their entire ecosystem).


