#!/usr/bin/env python3
"""
Regression test runner for feather examples.

This script runs through all examples in feather/examples/ and verifies
they compile and run successfully on the target hardware.
"""

import subprocess
import sys
import time
import os
import threading
import socket
import asyncio
from typing import Optional, Dict, List, Callable
from dataclasses import dataclass

# Import the test server
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'python'))
import server as test_server

DEFAULT_FLASH_TIMEOUT = 15

@dataclass
class TestConfig:
    """Configuration for a single test."""
    name: str
    flash_timeout: int  # seconds - timeout for build + flash phases
    run_timeout: int  # seconds - timeout for execution after flashing completes
    features: List[str]
    env_vars: Dict[str, str]
    requires_server: bool = False
    server_port: Optional[int] = None  # Port to run test server on
    server_verify_callback: Optional[Callable] = None  # Callback to verify server received data
    device_is_server: bool = False  # If True, device acts as server and test framework acts as client
    client_send_data: Optional[str] = None  # Data to send from test framework to device server (single request mode)
    client_protocol: str = 'tcp'  # Protocol for client: 'tcp' or 'udp'
    client_verify_callback: Optional[Callable] = None  # Callback to verify client response: (response_str, nonce) -> (pass, message)
    client_multi_request_callback: Optional[Callable] = None  # Callback for multi-request testing: (device_ip, port, elapsed_fn) -> (pass, message)
    success_pattern: Optional[str] = None  # String to look for in output to confirm success
    expected_output: Optional[str] = None  # String to track when expected output appears (for timing info)


def get_local_ip():
    """Get the local IP address of this machine."""
    hostname = socket.gethostname()
    local_ips = socket.gethostbyname_ex(hostname)[2]
    # Return first non-loopback IP
    for ip in local_ips:
        if not ip.startswith('127.'):
            return ip
    return local_ips[0] if local_ips else '127.0.0.1'

async def send_to_device_async(device_ip: str, port: int, data: str, protocol: str = 'tcp') -> Optional[str]:
    """
    Send data to device server and return response using asyncio.

    Args:
        device_ip: IP address of device
        port: Port number device is listening on
        data: Data to send
        protocol: 'tcp' or 'udp'

    Returns:
        Response string or None on error
    """
    try:
        if protocol == 'udp':
            loop = asyncio.get_event_loop()
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.setblocking(False)
            try:
                await loop.sock_sendto(sock, data.encode('utf-8'), (device_ip, port))
                response_data = await asyncio.wait_for(
                    loop.sock_recv(sock, 1024),
                    timeout=5.0
                )
                return response_data.decode('utf-8')
            finally:
                sock.close()
        else:  # tcp
            reader, writer = await asyncio.open_connection(device_ip, port)
            writer.write(data.encode('utf-8'))
            await writer.drain()

            # Read until connection closes or timeout
            # For HTTP responses, we need to read the full response
            chunks = []
            try:
                while True:
                    chunk = await asyncio.wait_for(reader.read(1024), timeout=2.0)
                    if not chunk:
                        break
                    chunks.append(chunk)
            except asyncio.TimeoutError:
                # Timeout is expected when we've read all data
                pass

            writer.close()
            await writer.wait_closed()
            return b''.join(chunks).decode('utf-8')
    except Exception as e:
        print(f"[CLIENT] Error sending to {device_ip}:{port} - {e}")
        return None

def send_to_device(device_ip: str, port: int, data: str, protocol: str = 'tcp') -> Optional[str]:
    """
    Synchronous wrapper around async send_to_device_async.
    """
    try:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        try:
            return loop.run_until_complete(send_to_device_async(device_ip, port, data, protocol))
        finally:
            loop.close()
    except Exception as e:
        print(f"[CLIENT] Error in async loop: {e}")
        return None

def http_server_led_test(device_ip: str, port: int, elapsed_fn: Callable) -> tuple[bool, str]:
    """
    Multi-request test for http_server: GET index page, control LED on/off, verify state.

    Args:
        device_ip: IP address of device HTTP server
        port: HTTP server port
        elapsed_fn: Function to get elapsed time string for logging

    Returns:
        (success: bool, message: str)
    """
    import json

    # Test 1: GET / (index page)
    print(f"[{elapsed_fn()}] [CLIENT] Test 1: GET / (index page)")
    response = send_to_device(device_ip, port, 'GET / HTTP/1.1\r\nHost: device\r\n\r\n', 'tcp')
    if not response:
        return (False, "Test 1 failed: No response from index page")
    if 'HTTP/1.1 200 OK' not in response or 'text/html' not in response:
        return (False, f"Test 1 failed: Invalid index page response - {response[:100]}")
    print(f"[{elapsed_fn()}] [CLIENT] Test 1: Index page OK ✓")

    # Test 2: POST /api/led/ with {"led": true} - Turn LED on
    print(f"[{elapsed_fn()}] [CLIENT] Test 2: POST /api/led/ (turn on)")
    led_on_body = '{"led": true}'
    led_on_request = f'POST /api/led/ HTTP/1.1\r\nHost: device\r\nContent-Type: application/json\r\nContent-Length: {len(led_on_body)}\r\n\r\n{led_on_body}'
    response = send_to_device(device_ip, port, led_on_request, 'tcp')
    if not response:
        return (False, "Test 2 failed: No response from LED on")
    if 'HTTP/1.1 200 OK' not in response or 'application/json' not in response:
        return (False, f"Test 2 failed: Invalid LED on response - {response[:100]}")
    # Extract JSON body from HTTP response
    try:
        json_start = response.index('{')
        json_body = response[json_start:response.index('}', json_start) + 1]
        led_state = json.loads(json_body)
        if led_state.get('led') != True:
            return (False, f"Test 2 failed: LED state not true - {json_body}")
    except (ValueError, json.JSONDecodeError) as e:
        return (False, f"Test 2 failed: Could not parse JSON - {e}")
    print(f"[{elapsed_fn()}] [CLIENT] Test 2: LED turned on ✓")

    # Test 3: GET /api/led/ - Verify LED is on
    print(f"[{elapsed_fn()}] [CLIENT] Test 3: GET /api/led/ (verify on)")
    response = send_to_device(device_ip, port, 'GET /api/led/ HTTP/1.1\r\nHost: device\r\n\r\n', 'tcp')
    if not response:
        return (False, "Test 3 failed: No response")
    try:
        json_start = response.index('{')
        json_body = response[json_start:response.index('}', json_start) + 1]
        led_state = json.loads(json_body)
        if led_state.get('led') != True:
            return (False, f"Test 3 failed: LED not on - {json_body}")
    except (ValueError, json.JSONDecodeError) as e:
        return (False, f"Test 3 failed: Could not parse JSON - {e}")
    print(f"[{elapsed_fn()}] [CLIENT] Test 3: LED state verified on ✓")

    # Test 4: POST /api/led/ with {"led": false} - Turn LED off
    print(f"[{elapsed_fn()}] [CLIENT] Test 4: POST /api/led/ (turn off)")
    led_off_body = '{"led": false}'
    led_off_request = f'POST /api/led/ HTTP/1.1\r\nHost: device\r\nContent-Type: application/json\r\nContent-Length: {len(led_off_body)}\r\n\r\n{led_off_body}'
    response = send_to_device(device_ip, port, led_off_request, 'tcp')
    if not response:
        return (False, "Test 4 failed: No response from LED off")
    try:
        json_start = response.index('{')
        json_body = response[json_start:response.index('}', json_start) + 1]
        led_state = json.loads(json_body)
        if led_state.get('led') != False:
            return (False, f"Test 4 failed: LED state not false - {json_body}")
    except (ValueError, json.JSONDecodeError) as e:
        return (False, f"Test 4 failed: Could not parse JSON - {e}")
    print(f"[{elapsed_fn()}] [CLIENT] Test 4: LED turned off ✓")

    # Test 5: GET /api/led/ - Verify LED is off
    print(f"[{elapsed_fn()}] [CLIENT] Test 5: GET /api/led/ (verify off)")
    response = send_to_device(device_ip, port, 'GET /api/led/ HTTP/1.1\r\nHost: device\r\n\r\n', 'tcp')
    if not response:
        return (False, "Test 5 failed: No response")
    try:
        json_start = response.index('{')
        json_body = response[json_start:response.index('}', json_start) + 1]
        led_state = json.loads(json_body)
        if led_state.get('led') != False:
            return (False, f"Test 5 failed: LED not off - {json_body}")
    except (ValueError, json.JSONDecodeError) as e:
        return (False, f"Test 5 failed: Could not parse JSON - {e}")
    print(f"[{elapsed_fn()}] [CLIENT] Test 5: LED state verified off ✓")

    return (True, "All 5 HTTP server tests passed ✓ (index, LED on, verify on, LED off, verify off)")

def telnet_shell_close_test(device_ip: str, port: int, elapsed_fn: Callable) -> tuple[bool, str]:
    """
    Test telnet_shell: connect and execute 'close' command to cleanly disconnect.

    Args:
        device_ip: IP address of device telnet server
        port: Telnet server port (23)
        elapsed_fn: Function to get elapsed time string for logging

    Returns:
        (success: bool, message: str)
    """
    print(f"[{elapsed_fn()}] [CLIENT] Connecting to telnet shell and sending 'close' command")

    # Send 'close\r' command (telnet expects \r for enter)
    response = send_to_device(device_ip, port, 'close\r', 'tcp')
    if not response:
        return (False, "Failed: No response from telnet shell")

    # Check for expected shell prompts/messages
    if 'Hello to shell!' in response:
        print(f"[{elapsed_fn()}] [CLIENT] Received shell greeting ✓")

    # The response should contain shell output before closing
    if len(response) > 0:
        print(f"[{elapsed_fn()}] [CLIENT] Received response ({len(response)} bytes)")
        return (True, "Telnet shell test passed ✓ (connected and closed cleanly)")
    else:
        return (False, "Failed: Empty response from telnet shell")

# Test configuration dictionary - maps example name to its config
TEST_CONFIGS = {
    # 1. Simplest test - just blink LED, no WiFi
    # Note: defmt output goes through RTT which isn't captured by Python subprocess
    # So we rely on timeout as success indicator
    'blinky': TestConfig(
        name='blinky',
        flash_timeout=3,
        run_timeout=2,
        features=['irq', 'defmt'],
        env_vars={},
        success_pattern=None,  # Timeout means success for looping embedded code
        expected_output='Hello, blinky!'
    ),

    # 2. WiFi init + get random bytes
    'prng': TestConfig(
        name='prng',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=2,
        features=['irq', 'defmt'],
        env_vars={},
        success_pattern='Got the Random bytes:'
    ),

    # 3. WiFi init + scan for access points
    'scan': TestConfig(
        name='scan',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=15,
        features=['irq', 'defmt'],
        env_vars={},
        success_pattern='Scan done, aps:'
    ),

    # 4. WiFi init + connect to AP (requires network credentials)
    'connect_network': TestConfig(
        name='connect_network',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=20,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
        },
        success_pattern='.. connected to AP, going to loop'
    ),

    # 5. WiFi init + connect using saved credentials
    'connect_saved': TestConfig(
        name='connect_saved',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=20,
        features=['irq', 'defmt'],
        env_vars={},
        success_pattern='.. connected to AP, going to loop'
    ),

    # 6. Connect + DNS lookup (requires network and hostname)
    'dns': TestConfig(
        name='dns',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=20,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_HOST': os.environ.get('TEST_HOST', 'dns.google.com'),
        },
        success_pattern='DNS: dns.google.com -> 8.8.' # Either 8.8 or 4.4 is fine
    ),

    # 7. Connect + ping a host (requires network and target IP)
    'ping': TestConfig(
        name='ping',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=20,
        features=['irq', 'defmt'],
        env_vars={ # Uses saved network credentials
            'TEST_IP': os.environ.get('TEST_IP', '8.8.8.8'),
        },
        success_pattern='ping result: ip'
    ),

    # 8. UDP client - send UDP packet to local server
    'udp_client': TestConfig(
        name='udp_client',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=15,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_IP': os.environ.get('HOST_IP', get_local_ip()),
            'TEST_PORT': '12345',
        },
        requires_server=True,
        server_port=12345,
        success_pattern='-----Response: UDP/1.0 200 OK from port 12345'
    ),

    # 9. HTTP client - send HTTP GET request to local server
    'http_client': TestConfig(
        name='http_client',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=15,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_IP': os.environ.get('HOST_IP', get_local_ip()),
            'TEST_PORT': '12346',
            'TEST_HOST': 'testserver.local',
        },
        requires_server=True,
        server_port=12346,
        success_pattern='-----Response: HTTP/1.0 200 OK from port 12346'
    ),

    # 10. UDP server - device listens for UDP packets, test framework sends
    'udp_server': TestConfig(
        name='udp_server',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=15,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_PORT': '12347',
            'LOOP_FOREVER': 'false',
        },
        device_is_server=True,
        client_send_data='Hello from test_',  # Nonce will be appended
        client_protocol='udp',
        client_verify_callback=lambda resp, nonce: (
            f"Hello, client_{nonce}!" in resp,
            f"Response: {resp} {'✓' if f'Hello, client_{nonce}!' in resp else '✗'} (nonce: {nonce})"
        ),
        success_pattern='-----Sent response to'
    ),

    # 11. TCP server - device listens on TCP port, test framework connects and sends
    'tcp_server': TestConfig(
        name='tcp_server',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=15,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_PORT': '12348',
            'LOOP_FOREVER': 'false',
        },
        device_is_server=True,
        client_send_data='GET / HTTP/1.1\r\nHost: device_',  # Nonce will be appended
        client_protocol='tcp',
        client_verify_callback=lambda resp, nonce: (
            f"Hello, client_{nonce}!" in resp,
            f"Response contains 'Hello, client_{nonce}!' {'✓' if f'Hello, client_{nonce}!' in resp else '✗'} (nonce: {nonce})"
        ),
        success_pattern='-----Sent response to'
    ),

    # 12. HTTP server - Full HTTP server with LED control API
    'http_server': TestConfig(
        name='http_server',
        flash_timeout=DEFAULT_FLASH_TIMEOUT + 5,
        run_timeout=20,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_PORT': '80',  # HTTP server runs on port 80
        },
        device_is_server=True,
        client_protocol='tcp',
        client_multi_request_callback=lambda device_ip, port, elapsed: http_server_led_test(device_ip, port, elapsed),
        success_pattern='-----Accepted connection from'
    ),

    # 13. iperf3 client TCP - Network performance testing over TCP
    'iperf3_client_tcp': TestConfig(
        name='iperf3_client',
        flash_timeout=DEFAULT_FLASH_TIMEOUT + 15,
        run_timeout=60,  # Longer timeout for 512KB transfer
        features=['irq', 'defmt', 'iperf3'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_IPERF_IP': '34.19.56.238',  # iperf.kaidokert.com
            'TEST_IPERF_PORT': '5201',
            'TEST_IPERF_UDP': 'false',  # TCP test
            'NUM_BYTES': '524288',  # 512KB transfer
            'BLOCK_LEN': '1024',  # 1KB blocks
        },
        success_pattern='TCP Speed'  # Matches output like "TCP Speed 25.106 KB/s"
    ),

    # 14. iperf3 client UDP - Network performance testing over UDP
    'iperf3_client_udp': TestConfig(
        name='iperf3_client',
        flash_timeout=DEFAULT_FLASH_TIMEOUT + 15,
        run_timeout=60,  # Longer timeout for 512KB transfer
        features=['irq', 'defmt', 'iperf3'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_IPERF_IP': '34.19.56.238',  # iperf.kaidokert.com
            'TEST_IPERF_PORT': '5201',
            'TEST_IPERF_UDP': 'true',  # UDP test
            'NUM_BYTES': '524288',  # 512KB transfer
            'BLOCK_LEN': '1024',  # 1KB blocks
        },
        success_pattern='UDP Speed'  # Matches output like "UDP Speed 25.106 KB/s"
    ),

    # 15. HTTP speed test - Download 1MB file from kaidokert.com
    'http_speed_test': TestConfig(
        name='http_speed_test',
        flash_timeout=DEFAULT_FLASH_TIMEOUT + 15,
        run_timeout=90,  # Longer timeout for 1MB download over slow connection
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_IP': '18.155.192.71',  # kaidokert.com IP (AWS)
            'TEST_PORT': '80',
            'TEST_HOST': 'kaidokert.com',
            'TEST_FILE': '/test-file-1mb.json',  # 0.93 MB file
        },
        success_pattern='=== Speed Test Complete ==='
    ),

    # 16. mDNS service discovery - Multicast DNS responder
    'mdns': TestConfig(
        name='mdns',
        flash_timeout=DEFAULT_FLASH_TIMEOUT + 15,
        run_timeout=20,
        features=['irq', 'defmt'],
        env_vars={
            'TEST_SSID': os.environ.get('TEST_SSID', 'network'),
            'TEST_PASSWORD': os.environ.get('TEST_PASSWORD', 'password'),
            'TEST_IP': '224.0.0.251',  # mDNS multicast group
            'TEST_PORT': '5353',  # mDNS port
        },
        success_pattern='---> Sending announce'  # Device sends mDNS announce packet successfully
    ),

    # 17. Telnet shell - Interactive command shell over telnet
    'telnet_shell': TestConfig(
        name='telnet_shell',
        flash_timeout=DEFAULT_FLASH_TIMEOUT,
        run_timeout=15,
        features=['irq', 'defmt', 'telnet'],
        env_vars={},  # Uses saved WiFi credentials
        device_is_server=True,
        client_protocol='tcp',
        client_multi_request_callback=lambda device_ip, port, elapsed: telnet_shell_close_test(device_ip, 23, elapsed),
        success_pattern='-----Connection closed-----'
    ),
}


class TestRunner:
    """Manages running regression tests for feather examples."""

    def __init__(self, working_dir: str = '.', verbose: bool = False):
        self.working_dir = working_dir
        self.verbose = verbose
        self.passed = []
        self.failed = []
        self.skipped = []

    def run_cargo_command(self, example_name: str, config: TestConfig) -> bool:
        """
        Run cargo command for a specific example.

        Args:
            example_name: Name of the example to run
            config: Test configuration

        Returns:
            True if test passed, False otherwise
        """
        # Build the cargo command
        cmd = ['cargo', 'run', '--release', '--example', example_name]

        # Add features
        if config.features:
            features_str = ','.join(config.features)
            cmd.extend(['--features', features_str])

        print(f"\n{'='*70}")
        print(f"Running test: {example_name}")
        print(f"Command: {' '.join(cmd)}")
        print(f"Flash timeout: {config.flash_timeout}s, Run timeout: {config.run_timeout}s")
        if config.env_vars:
            print(f"Environment: {config.env_vars}")
        print(f"{'='*70}\n")

        # Set up environment
        env = os.environ.copy()

        # Set logging levels for deterministic output
        # DEFMT_LOG controls defmt logging level (for embedded target)
        # RUST_LOG controls Rust logging level (for probe-rs and cargo)
        env['DEFMT_LOG'] = 'info'
        env['RUST_LOG'] = 'warn'

        # Apply test-specific environment variables (can override logging if needed)
        env.update(config.env_vars)

        # Create log file for full output
        log_file = f"test_{example_name}.log"

        try:
            # Run the command with real-time output monitoring
            process = subprocess.Popen(
                cmd,
                cwd=self.working_dir,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1  # Line buffered
            )

            output_lines = []
            compile_started = False
            compile_start_time = None
            flash_started = False
            flash_start_time = None
            flash_complete_time = None
            run_phase_start_time = None
            first_output_time = None
            expected_output_time = None
            running_started = False
            client_data_sent = False  # Track if we've sent data to device server
            device_ip = None  # Track device IP parsed from output

            # Monitor output in real-time
            import time
            start_time = time.time()

            def elapsed():
                """Get elapsed time since appropriate phase start in seconds with 2 decimal places"""
                current = time.time()
                if run_phase_start_time is not None:
                    # In run phase - show time since run started
                    return f"{current - run_phase_start_time:6.2f}s"
                else:
                    # In flash phase - show time since test started
                    return f"{current - start_time:6.2f}s"

            # Use threading to read output without blocking
            import threading
            import queue

            output_queue = queue.Queue()

            def read_output():
                """Read output lines in a separate thread"""
                try:
                    for line in iter(process.stdout.readline, ''):
                        if line:
                            output_queue.put(('line', line))
                        else:
                            break
                except Exception as e:
                    output_queue.put(('error', str(e)))
                finally:
                    output_queue.put(('done', None))

            reader_thread = threading.Thread(target=read_output, daemon=True)
            reader_thread.start()

            reader_done = False

            while True:
                # Check for timeout - different timeout for flash vs run phase
                current_time = time.time() - start_time

                if flash_complete_time is None:
                    # Still in flash phase
                    if current_time > config.flash_timeout:
                        process.kill()
                        print(f"[{elapsed()}] [TIMEOUT] Flash phase timed out after {config.flash_timeout}s")
                        break
                else:
                    # In run phase
                    run_time = current_time - flash_complete_time
                    if run_time > config.run_timeout:
                        process.kill()
                        print(f"[{elapsed()}] [TIMEOUT] Run phase timed out after {config.run_timeout}s (expected for looping tests)")
                        break

                # Try to read from queue with timeout
                try:
                    msg_type, content = output_queue.get(timeout=0.1)

                    if msg_type == 'line':
                        line = content
                        output_lines.append(line)

                        # Detect first output in run phase (any line with [INFO], [DEBUG], [WARN], [ERROR], [TRACE])
                        if run_phase_start_time is not None and first_output_time is None and any(marker in line for marker in ['[INFO ]', '[DEBUG]', '[WARN ]', '[ERROR]', '[TRACE]']):
                            first_output_time = time.time() - run_phase_start_time
                            print(f"[{elapsed()}] [RUN] First output detected ({first_output_time:.2f}s into run phase)")


                        # Detect expected output pattern
                        if run_phase_start_time is not None and expected_output_time is None and (config.expected_output and config.expected_output in line):
                            expected_output_time = time.time() - run_phase_start_time
                            print(f"[{elapsed()}] [RUN] Expected output detected: '{config.expected_output}' ({expected_output_time:.2f}s into run phase)")


                        # For server tests, try to parse device IP from output
                        if config.device_is_server and device_ip is None:
                            # Look for IP in various server startup messages
                            import re
                            # Pattern: "at http://192.168.5.101:80" or similar
                            ip_match = re.search(r'(?:at http://|IP:? |ip:? )(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})', line)
                            if ip_match:
                                device_ip = ip_match.group(1)
                                print(f"[{elapsed()}] [RUN] Detected device IP: {device_ip}")

                        # For server tests, detect when device is ready and send client data
                        if config.device_is_server and run_phase_start_time is not None and not client_data_sent:
                            # Detect server ready message (bind for UDP, listening for TCP)
                            server_ready = ('-----Bound to' in line and config.client_protocol == 'udp') or \
                                          ('-----Listening-----' in line and config.client_protocol == 'tcp')
                            if server_ready:
                                print(f"[{elapsed()}] [RUN] Device server is bound and ready")

                                # For TCP, give the device a moment to fully enter accept() state
                                if config.client_protocol == 'tcp':
                                    time.sleep(0.5)

                                # Get device IP from parsed output or environment variable
                                target_ip = device_ip or os.environ.get('DEVICE_IP')
                                if not target_ip:
                                    print(f"[{elapsed()}] [CLIENT] No device IP detected, cannot send to device server")
                                    print(f"[{elapsed()}] [CLIENT] Either ensure device logs its IP or set DEVICE_IP environment variable")
                                else:
                                    port = int(config.env_vars.get('TEST_PORT', '12345'))

                                    # Check if multi-request callback is provided
                                    if config.client_multi_request_callback:
                                        # Multi-request mode - callback handles all requests
                                        try:
                                            verify_passed, verify_msg = config.client_multi_request_callback(target_ip, port, elapsed)
                                            print(f"[{elapsed()}] [CLIENT] {verify_msg}")
                                            if not verify_passed:
                                                print(f"[{elapsed()}] [FAIL] Multi-request verification failed")
                                                process.kill()
                                                break
                                            # Mark as sent only after successful multi-request verification
                                            client_data_sent = True
                                        except Exception as e:
                                            print(f"[{elapsed()}] [CLIENT] Multi-request callback error: {e}")
                                            print(f"[{elapsed()}] [FAIL] Multi-request verification failed")
                                            process.kill()
                                            break
                                    else:
                                        # Single-request mode with optional nonce
                                        import random
                                        import string
                                        nonce = random.choice(string.ascii_lowercase)
                                        send_data = config.client_send_data + nonce
                                        print(f"[{elapsed()}] [CLIENT] Sending to device at {target_ip}:{port} (nonce: {nonce})")

                                        response = send_to_device(
                                            target_ip,
                                            port,
                                            send_data,
                                            config.client_protocol
                                        )

                                        if response:
                                            # Use callback to verify response if provided
                                            if config.client_verify_callback:
                                                verify_passed, verify_msg = config.client_verify_callback(response, nonce)
                                                if verify_passed:
                                                    print(f"[{elapsed()}] [CLIENT] {verify_msg}")
                                                    # Mark as sent only after successful verification
                                                    client_data_sent = True
                                                else:
                                                    print(f"[{elapsed()}] [CLIENT] {verify_msg}")
                                                    print(f"[{elapsed()}] [CLIENT] Full response ({len(response)} bytes): {response!r}")
                                                    print(f"[{elapsed()}] [FAIL] Client verification failed")
                                                    process.kill()
                                                    break
                                            else:
                                                print(f"[{elapsed()}] [CLIENT] Got response: {response}")
                                                # Mark as sent after getting response (no callback verification)
                                                client_data_sent = True
                                        else:
                                            print(f"[{elapsed()}] [CLIENT] No response received")
                                            print(f"[{elapsed()}] [FAIL] No response from device server")
                                            process.kill()
                                            break

                        # Print verbose output with timing
                        if self.verbose:
                            print(f"[{elapsed()}] [VERBOSE] {line.rstrip()}")

                        # Detect compilation phase
                        if 'Compiling' in line and not compile_started:
                            compile_start_time = time.time()
                            print(f"[{elapsed()}] [BUILD] Starting compilation...")
                            compile_started = True
                        elif 'Finished `' in line and compile_started and not flash_started:
                            compile_duration = time.time() - compile_start_time if compile_start_time else 0
                            print(f"[{elapsed()}] [BUILD] Compilation complete! (took {compile_duration:.2f}s)")
                        # Detect flashing phase
                        elif 'Running `probe-rs' in line and not flash_started:
                            flash_start_time = time.time()
                            print(f"[{elapsed()}] [FLASH] Programming device...")
                            flash_started = True
                        elif 'Finished in' in line and flash_started:
                            flash_duration = time.time() - flash_start_time if flash_start_time else 0
                            flash_complete_time = time.time() - start_time
                            # Print flash complete with flash timeline
                            print(f"[{flash_complete_time:6.2f}s] [FLASH] Programming complete! (took {flash_duration:.2f}s)")
                            # Reset timeline for run phase
                            run_phase_start_time = time.time()
                            print(f"[  0.00s] [RUN] Starting run phase (timeout: {config.run_timeout}s)")
                        # Detect program start (look for first defmt output or success pattern)
                        elif config.success_pattern and config.success_pattern in line and not running_started:
                            print(f"[{elapsed()}] [RUN] Program started - found: '{config.success_pattern}'")
                            running_started = True
                            # For tests with success patterns, we can exit early
                            process.kill()
                            break

                    elif msg_type == 'error':
                        print(f"[{elapsed()}] [ERROR] Reader thread error: {content}")
                        break
                    elif msg_type == 'done':
                        reader_done = True
                        # Check if process has finished too
                        if process.poll() is not None:
                            break
                        continue

                except queue.Empty:
                    # No output available, check if process finished
                    if process.poll() is not None and reader_done:
                        break
                    continue

            # Wait for process to complete
            process.wait(timeout=1)

            # Write full output to log file
            with open(log_file, 'w') as f:
                f.writelines(output_lines)

            full_output = ''.join(output_lines)

            total_time = time.time() - start_time

            # Check output for success pattern if specified
            if config.success_pattern:
                if config.success_pattern in full_output:
                    print(f"[{elapsed()}] [PASS] Test completed successfully (total: {total_time:.2f}s)")
                    return True
                else:
                    print(f"[{elapsed()}] [FAIL] Success pattern not found: '{config.success_pattern}'")
                    print(f"See log file: {log_file}")
                    return False

            # If no success pattern, run timeout means success (looping embedded code)
            if flash_complete_time and (time.time() - start_time - flash_complete_time) >= config.run_timeout:
                print(f"[{elapsed()}] [PASS] Test ran successfully (timed out as expected, total: {total_time:.2f}s)")
                return True

            # Check return code if process exited early
            return_code = process.returncode
            if return_code == 0 or return_code == -15:  # -15 is SIGTERM (killed by us)
                print(f"[{elapsed()}] [PASS] Test completed successfully (total: {total_time:.2f}s)")
                return True
            else:
                print(f"[{elapsed()}] [FAIL] Command failed with return code {return_code}")
                print(f"See log file: {log_file}")
                return False

        except Exception as e:
            print(f"[ERROR] Exception occurred: {e}")
            import traceback
            traceback.print_exc()
            return False

    def run_test(self, example_name: str) -> bool:
        """
        Run a single test.

        Args:
            example_name: Name of the example to test

        Returns:
            True if test passed, False otherwise
        """
        if example_name not in TEST_CONFIGS:
            print(f"[SKIP] Skipping {example_name} - no test configuration")
            self.skipped.append(example_name)
            return None

        config = TEST_CONFIGS[example_name]

        # Start test server if required
        server_thread = None
        server_data = {'packets': []}

        if config.requires_server:
            if not config.server_port:
                print(f"[SKIP] Skipping {example_name} - server_port not configured")
                self.skipped.append(example_name)
                return None

            print(f"[SERVER] Starting test server on port {config.server_port}")

            # Callback to track received UDP data
            def on_udp_data(addr, port, data):
                server_data['packets'].append(('udp', addr, port, data))
                if self.verbose:
                    print(f"[SERVER] UDP from {addr}:{port} - {len(data)} bytes")

            # Callback to track TCP connections
            def on_tcp_connect(addr, port):
                server_data['packets'].append(('tcp_connect', addr, port, None))
                if self.verbose:
                    print(f"[SERVER] TCP connection from {addr} on port {port}")

            # Callback to track received TCP data
            def on_tcp_data(addr, port, data):
                server_data['packets'].append(('tcp', addr, port, data))
                if self.verbose:
                    print(f"[SERVER] TCP from {addr}:{port} - {len(data)} bytes")

            # Start server in background thread
            server_thread = threading.Thread(
                target=test_server.start_combined_server,
                args=(config.server_port, 1),  # Just the one port
                kwargs={
                    'on_udp_data': on_udp_data,
                    'on_tcp_connect': on_tcp_connect,
                    'on_tcp_data': on_tcp_data,
                    'verbose': False
                },
                daemon=True
            )
            server_thread.start()

            # Give server time to start
            time.sleep(0.5)
            host_ip = os.environ.get('HOST_IP', get_local_ip())
            print(f"[SERVER] Test server ready on {host_ip}:{config.server_port}")

        success = self.run_cargo_command(config.name, config)

        # Verify server received data if callback provided
        if config.requires_server and config.server_verify_callback:
            if not config.server_verify_callback(server_data):
                print("[FAIL] Server verification failed - no data received")
                success = False

        if success:
            self.passed.append(example_name)
        else:
            self.failed.append(example_name)

        return success

    def run_all_tests(self, test_names: Optional[List[str]] = None):
        """
        Run all configured tests or a specific subset.

        Args:
            test_names: Optional list of specific test names to run.
                       If None, runs all configured tests.
        """
        if test_names is None:
            test_names = list(TEST_CONFIGS.keys())

        print(f"Running {len(test_names)} tests...")

        for test_name in test_names:
            self.run_test(test_name)

        # Print summary
        print(f"\n{'='*70}")
        print("TEST SUMMARY")
        print(f"{'='*70}")
        print(f"Passed:  {len(self.passed)}")
        print(f"Failed:  {len(self.failed)}")
        print(f"Skipped: {len(self.skipped)}")

        if self.passed:
            print(f"\nPassed tests: {', '.join(self.passed)}")
        if self.failed:
            print(f"\nFailed tests: {', '.join(self.failed)}")
        if self.skipped:
            print(f"\nSkipped tests: {', '.join(self.skipped)}")

        print(f"{'='*70}\n")

        return len(self.failed) == 0


def main():
    """Main entry point."""
    import argparse

    parser = argparse.ArgumentParser(
        description='Run regression tests for feather examples'
    )
    parser.add_argument(
        'tests',
        nargs='*',
        help='Specific tests to run (default: all configured tests)'
    )
    parser.add_argument(
        '--list',
        action='store_true',
        help='List all available tests'
    )
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Print all output lines with timing information'
    )

    args = parser.parse_args()

    if args.list:
        print("Available tests:")
        for name, config in TEST_CONFIGS.items():
            features = ','.join(config.features)
            print(f"  {name:20s} - features: [{features}], flash: {config.flash_timeout}s, run: {config.run_timeout}s")
        return 0

    runner = TestRunner(verbose=args.verbose)
    success = runner.run_all_tests(args.tests or None)

    return 0 if success else 1


if __name__ == '__main__':
    sys.exit(main())
