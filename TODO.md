## Development notes

- In Client, implement chip start, connect, scan
    - [x] Chip start
    - [x] Connect to AP
    - [x] Scan
    - [x] Connect with saved credentials
- [x] Fix the module start wait
- Clean up data passing in client events
    - Maybe get rid of EventListener entirely
- [x] Refactor dispatch_events to be internal, don't expose to clients
- [x] Clippy clean
- Feature Parity with Wifi101 lib:
 - [ ] Add Station/provisioning mode
 - [ ] Implement missing SSL stuff
 - [ ] Implement firmware update
 - [ ] Implement all unimplemented `WifiRequest` commands
       This also needs a design for top-level config APIs
       WPS config is not important - ignore
- [ ] Write docs to toplevel APIs - Work in progress
- [ ] Feature gate udp / tcp to make the binary smaller
- [ ] Investigate and reduce bloat a bit more
- [x] Get github checks properly running
- [x] Add badges
- [x] Add missing coverage
- [ ] Implement an async version of this
- Add missing tests
- Stress test, fast downloads ( iperf3 ), multi-socket use cases
- Make it work with Arduino/Adafruit bootloader
