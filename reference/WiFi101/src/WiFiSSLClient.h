// <license>
#ifndef WIFISSLCLIENT_H
#define WIFISSLCLIENT_H

#include "WiFiClient.h"

class WiFiSSLClient : public WiFiClient {

public:
	WiFiSSLClient();
	WiFiSSLClient(uint8_t sock);

	virtual int connect(IPAddress ip, uint16_t port);
	virtual int connect(const char* host, uint16_t port);
};

#endif /* WIFISSLCLIENT_H */
