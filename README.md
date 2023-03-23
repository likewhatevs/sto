# sto

![ci badge](https://github.com/likewhatevs/sto/actions/workflows/rust.yml/badge.svg)

This repo contains a server, cli and (super basic) UI to store/retrieve profiler data in an efficient manner. There are also a couple of cool DB functions that leverage the properties of the schema.

The main purpose of this repo is to share ideas. I'll link some slides here that explain this more, but TL;DR you can do cool stuff if you represent profiler data in a DAG (which it is) so this does that.

The cli stores/uploads profiler data to postgres, via the server. The cli profiles a provided locally running pid (via libbpf), symbolizes it (via blazesym) and posts it to the server. The interesting part of the server is more/less [this](https://github.com/likewhatevs/sto/blob/f160f9e2f28bf5af815fc0079eb20c298913186c/src/bin/server.rs#L196-L277). The CLI is all kinda interesting, but [also short](https://github.com/likewhatevs/sto/blob/main/src/bin/cli.rs). There's also a couple of cool DB functions [here](https://github.com/likewhatevs/sto/blob/6bfc8555001debb50efc1f25757781ff6b9b14b2/migrations/20230305040305_init.up.sql#L43-L117).

The UI (templated html web page which GET's some json from the server) shows these stored profiles, along with the capacity savings the storage approach used by this repo uses.

The cli and server are buildable via `cargo build --release`.

Here's a screenshot of the UI:

<img width="1319" alt="CleanShot 2023-03-13 at 02 34 05@2x" src="https://user-images.githubusercontent.com/12107998/225395658-528dfdb7-5d5c-4080-81d2-d0e99c2a7da5.png">

### How to run this all locally and see how this works for your data.

The easiest way is to use something like gentoo or arch to have the latest-and-greatest kernel and everything else running locally. Ignoring the incredibly important detail of kernel version, the dependencies are all in .devcontainer, in either Dockerfile.db or Dockerfile.

### (future-ish) quickstart/live demo.
#### Note -- this does not work *yet*, codespaces are running an older kernel than this needs, kernel req TBD (I think 5.19 may work but need to confirm).
#### I will upload data from a gentoo machine to enable this to be a read-only live-demo with real-yet-contrived data until codespaces are running a new-enough kernel to enable profiling via libbpf in them.

1) Fork it via:
<img width="151" alt="CleanShot 2023-03-23 at 00 34 22@2x" src="https://user-images.githubusercontent.com/12107998/227104970-4635263c-bc2c-4b30-821b-8a99ddf4388c.png">

2) On your fork, open a code-space of it in your browser via:
<img width="440" alt="CleanShot 2023-03-23 at 00 34 08@2x" src="https://user-images.githubusercontent.com/12107998/227105047-98e57748-219d-4ca0-9bd1-b410952b7346.png">

3) Build the server and cli by entering the following into the terminal of your code space as the following image shows:
```
cargo build --release
```
![CleanShot 2023-03-23 at 00 45 29@2x](https://user-images.githubusercontent.com/12107998/227105684-1d41d410-f134-4cb3-89be-31f0066963a9.png)

4) Start the server via the following command, and open a new terminal via the indicated `+` button:
```
./target/release/server
```
![CleanShot 2023-03-23 at 00 53 08@2x](https://user-images.githubusercontent.com/12107998/227106830-04bb1453-f422-4c4a-b5aa-a389b3a82851.png)


5) In this new tab, built and run a binary to test via the following command (or just use htop/whatever and get it's pid, etc.):
```
cd demo && make && ./demo
```
![CleanShot 2023-03-23 at 00 55 30@2x](https://user-images.githubusercontent.com/12107998/227107130-aa3876c0-166f-47e7-9154-ec6fea1fbe2d.png)

6) Copy the numbers printed (the demo binary's PID) and open a new terminal via the `+` button indicated:
![CleanShot 2023-03-23 at 00 56 14@2x](https://user-images.githubusercontent.com/12107998/227107257-1de5627d-297f-4481-bd6a-5ae72631ca98.png)

7) In this new terminal, profile the demo binary and send data to the server via the following command, then click the ports button:
```
./target/release/cli --version one-or-whatever --binary demo-or-whatever --pid 32651
```
![CleanShot 2023-03-23 at 00 59 02@2x](https://user-images.githubusercontent.com/12107998/227128213-5d5ce285-852c-4317-b3db-f1c4532ff1b9.png)

8) Open the UI via the following button to see the application being profiled:
![CleanShot 2023-03-23 at 01 47 23@2x](https://user-images.githubusercontent.com/12107998/227128501-dae10b2b-0916-4409-ab57-cce31f6cae94.png)

### Cool things used here
* https://github.com/hodgesds/bpftune -- profiler to generate audio from stack snapshots so you can hear the sounds of all your polling and locks.
* https://github.com/libbpf/libbpf-rs -- enable using bpf profilers via rust easy because rust makes some things easier.
* https://github.com/libbpf/blazesym -- deal with the hard problem of symbolization for me because it's really hard. Also in rust.
* lots of random copy pastes from the internet that I don't recall atm lol.
