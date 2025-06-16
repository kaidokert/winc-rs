// <license>
#ifndef FIRMWARE_PROGRAMMER_APIS_H_INCLUDED
#define FIRMWARE_PROGRAMMER_APIS_H_INCLUDED

#include "common/include/nm_common.h"
#include "programmer/programmer.h"
#include "spi_flash/include/spi_flash_map.h"

#define programmer_write_cert_image(buff)   programmer_write((uint8*)buff, M2M_TLS_FLASH_ROOTCERT_CACHE_OFFSET, M2M_TLS_FLASH_ROOTCERT_CACHE_SIZE)
#define programmer_read_cert_image(buff)    programmer_read((uint8*)buff, M2M_TLS_FLASH_ROOTCERT_CACHE_OFFSET, M2M_TLS_FLASH_ROOTCERT_CACHE_SIZE)
#define programmer_erase_cert_image()       programmer_erase(M2M_TLS_FLASH_ROOTCERT_CACHE_OFFSET, M2M_TLS_FLASH_ROOTCERT_CACHE_SIZE)

#define programmer_write_firmware_image(buff,offSet,sz) programmer_write((uint8*)buff, offSet, sz)
#define programmer_read_firmware_image(buff,offSet,sz)  programmer_read((uint8*)buff, offSet, sz)

#define programmer_erase_all()               programmer_erase(0, programmer_get_flash_size())

#endif /* FIRMWARE_PROGRAMMER_APIS_H_INCLUDED */
