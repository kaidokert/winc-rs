// <license>
#include "WiFiSSLClient.h"

WiFiSSLClient::WiFiSSLClient() :
	WiFiClient()
{
}

WiFiSSLClient::WiFiSSLClient(uint8_t sock) :
	WiFiClient(sock)
{
}

int WiFiSSLClient::connect(IPAddress ip, uint16_t port)
{
	return WiFiClient::connectSSL(ip, port);
}

int WiFiSSLClient::connect(const char* host, uint16_t port)
{
	return WiFiClient::connectSSL(host, port);
}
