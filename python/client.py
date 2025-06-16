import asyncio
import argparse
import logging
import sys
import socket

log = logging.getLogger(__name__)

async def send_tcp_request(host, port, request_data):
    """Send TCP request using asyncio streams"""
    log.info(f"TCP: Connecting to {host}:{port} for {request_data[:50]}...")
    reader, writer = await asyncio.open_connection(host, port)

    # Send your custom request data
    writer.write(request_data.encode('utf-8'))
    await writer.drain()  # Ensure the data is sent

    # Read the response (adjust buffer size or logic as needed)
    response = await reader.read(1024)  # Read up to 1024 bytes

    # Close the connection
    writer.close()
    await writer.wait_closed()

    return response.decode('utf-8')

async def send_udp_request(host, port, request_data):
    """Send UDP request using asyncio datagram protocol"""
    log.info(f"UDP: Sending to {host}:{port} for {request_data[:50]}...")

    # Create UDP socket
    loop = asyncio.get_event_loop()

    # Create a UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setblocking(False)

    try:
        # Send the data
        await loop.sock_sendto(sock, request_data.encode('utf-8'), (host, port))

        # Receive the response (with timeout)
        try:
            response_data = await asyncio.wait_for(
                loop.sock_recv(sock, 1024),
                timeout=5.0  # 5 second timeout
            )
            return response_data.decode('utf-8')
        except asyncio.TimeoutError:
            log.warning(f"UDP request to {host}:{port} timed out")
            return "TIMEOUT"
    finally:
        sock.close()

async def send_socket_request(host, port, request_data, protocol='tcp'):
    """Send request using specified protocol"""
    if protocol.lower() == 'udp':
        return await send_udp_request(host, port, request_data)
    else:
        return await send_tcp_request(host, port, request_data)

async def main():
    parser = argparse.ArgumentParser(description='Send socket requests to a server')
    parser.add_argument('--host', type=str, default='localhost', help='Host address')
    parser.add_argument('--port', type=int, default=80, help='Port number')
    parser.add_argument('-u', '--udp', action='store_true', help='Use UDP protocol (default is TCP)')
    parser.add_argument('--requests', type=int, default=5, help='Number of requests to send')
    parser.add_argument('--log_level', choices=["INFO", "DEBUG"], default="INFO", help='Log level')
    args = parser.parse_args()

    host = args.host
    port = args.port
    protocol = 'udp' if args.udp else 'tcp'

    logging.basicConfig(level=args.log_level, stream=sys.stdout)

    # Create different request formats based on protocol
    if protocol == 'udp':
        # For UDP, use simpler request format (no HTTP headers needed for basic UDP server)
        requests = [f"UDP request {i}" for i in range(1, args.requests + 1)]
    else:
        # For TCP, use HTTP format
        requests = [f"GET /?req={i} HTTP/1.1\r\nHost: {args.host}\r\n\r\n" for i in range(1, args.requests + 1)]

    log.info(f"Sending {args.requests} {protocol.upper()} requests to {host}:{port}")

    # Create tasks for all socket requests
    tasks = [send_socket_request(host, port, req, protocol) for req in requests]

    # Process results as they complete
    for i, task in enumerate(asyncio.as_completed(tasks), 1):
        result = await task  # Get the result as soon as this task finishes
        print(f"Result {i}: {result[:100]}...")

# Run the event loop
if __name__ == "__main__":
    asyncio.run(main())
