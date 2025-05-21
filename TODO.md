## Development notes

- In Client, implement chip start, connect, scan
    - [x] Chip start
    - [x] Connect to AP
    - [x] Scan
    - [x] Connect with saved credentials
- [x] Fix the module start wait
- [ ] Clean up data passing in client events
    - Maybe get rid of EventListener entirely
- [x] Refactor dispatch_events to be internal, don't expose to clients
- [x] Clippy clean
- Feature Parity with Wifi101 lib:
 - [x] Add Station/provisioning mode
 - [ ] Implement missing SSL stuff
 - [ ] Implement firmware update
 - [ ] Implement all unimplemented `WifiRequest` commands
       This also needs a design for top-level config APIs
       WPS config is not important - ignore
- [ ] Write docs to toplevel APIs - Work in progress
- [ ] Feature gate udp / tcp to make the binary smaller
- [ ] Investigate and reduce bloat a bit more
- [ ] Implement a simple telnet command shell for testing/demos ( e.g. blinky, iperf )
      Basic Telnet exists - but it's slow and doesn't expose any board commands
- [x] Get github checks properly running
- [x] Add badges
- [x] Add missing coverage
- Add missing tests
- Stress test, fast downloads ( iperf3 ), multi-socket use cases
   [x] Basic iperf3 TCP client is running, at up to 300 kbit/s ( packetsize=8192 )
   [ ] Add iperf3 UDP mode
   [ ] Figure out where the speed bottleneck is and try increase
   [ ] Add multi-stream test mode ( up to 7 TCP, 3 UDP sockets )
- [ ] Make it work with Arduino/Adafruit bootloader ( at 8 kilobyte start address )
- [ ] Implement an async version of this - WIP, initial demo added
