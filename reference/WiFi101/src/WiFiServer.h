// <license>
#ifndef WIFISERVER_H
#define WIFISERVER_H

#include <Arduino.h>
#include <Server.h>

class WiFiClient;

class WiFiServer : public Server {

private:
	SOCKET _socket;
	uint16_t _port;
	uint8_t begin(uint8_t opt);

public:
	WiFiServer(uint16_t);
	WiFiClient available(uint8_t* status = NULL);
	void begin();
	uint8_t beginSSL();
	virtual size_t write(uint8_t);
	virtual size_t write(const uint8_t *buf, size_t size);
	uint8_t status();

	using Print::write;

};

#endif /* WIFISERVER_H */
