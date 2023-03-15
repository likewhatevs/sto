# sto

This repo contains a server, cli and UI to store/retrieve profiler data.

The cli stores/uploads profiler data to postgres, via the server. The cli profiles a provided locally running pid (via libbpf), symbolizes it (via blazesym) and posts it to the server. The interesting part of the server is more/less [this](https://github.com/likewhatevs/sto/blob/f160f9e2f28bf5af815fc0079eb20c298913186c/src/bin/server.rs#L196-L277). The CLI is kinda all interesting, but [also short](https://github.com/likewhatevs/sto/blob/main/src/bin/cli.rs).

The UI (templated html web page which GET's some json from the server) shows these stored profiles, along with the capacity savings the storage approach used by this repo uses.

The cli and server are buildable via `cargo build --release`.

The server and database are deployable via docker-compose (which is to be added after reading today's news about docker breaking their entire ecosystem).

Here's a screenshot of the UI:

<img width="1319" alt="CleanShot 2023-03-13 at 02 34 05@2x" src="https://user-images.githubusercontent.com/12107998/225395658-528dfdb7-5d5c-4080-81d2-d0e99c2a7da5.png">
