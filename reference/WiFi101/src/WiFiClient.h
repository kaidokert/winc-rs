// <license>
#ifndef WIFICLIENT_H
#define WIFICLIENT_H

#include <Arduino.h>
#include <Client.h>
#include <IPAddress.h>

extern "C" {
	#include "socket/include/socket.h"
}

class WiFiClient : public Client {

public:
	WiFiClient();
	WiFiClient(uint8_t sock);

	uint8_t status();

	int connectSSL(IPAddress ip, uint16_t port);
	int connectSSL(const char* host, uint16_t port);
	virtual int connect(IPAddress ip, uint16_t port);
	virtual int connect(const char* host, uint16_t port);
	virtual size_t write(uint8_t);
	virtual size_t write(const uint8_t *buf, size_t size);
	virtual int available();
	virtual int read();
	virtual int read(uint8_t *buf, size_t size);
	virtual int peek();
	virtual void flush();
	virtual void stop();
	virtual uint8_t connected();
	virtual operator bool();
	bool operator==(const WiFiClient &other) const;
	bool operator!=(const WiFiClient &other) const;

	using Print::write;

	virtual IPAddress remoteIP();
	virtual uint16_t remotePort();

private:
	SOCKET _socket;

	int connect(const char* host, uint16_t port, uint8_t opt);
	int connect(IPAddress ip, uint16_t port, uint8_t opt, const uint8_t *hostname);
};

#endif /* WIFICLIENT_H */
