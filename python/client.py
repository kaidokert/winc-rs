import asyncio
import argparse
import logging
import sys

log = logging.getLogger(__name__)

async def send_socket_request(host, port, request_data):
    # Open a connection to the host:port
    log.info(f"Connecting to {host}:{port} for {request_data}")
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

async def main():
    parser = argparse.ArgumentParser(description='Send socket requests to a server')
    parser.add_argument('--host', type=str, default='localhost', help='Host address')
    parser.add_argument('--port', type=int, default=80, help='Port number')
    parser.add_argument('--requests', type=int, default=5, help='Number of requests to send')
    parser.add_argument('--log_level', choices=["INFO", "DEBUG"] , default="INFO", help='Log level')
    args = parser.parse_args()

    host = args.host
    port = args.port

    logging.basicConfig(level=args.log_level,stream=sys.stdout)

    requests = [f"GET /?req={i} HTTP/1.1\r\nHost: {args.host}\r\n\r\n" for i in range(1, args.requests + 1)]

    # Create tasks for all socket requests
    tasks = [send_socket_request(host, port, req) for req in requests]

    for task in asyncio.as_completed(tasks):
        result = await task  # Get the result as soon as this task finishes
        # Find the corresponding request (since order isn't guaranteed)
        # We could use a dict or tuple in tasks to map requests to results, but for simplicity:
        print(f"Result from a request: {result[:50]}...")

# Run the event loop
if __name__ == "__main__":
    asyncio.run(main())
