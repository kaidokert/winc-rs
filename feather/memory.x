MEMORY
{
  /*project*/
  FLASH (rx) : ORIGIN = 0x00000000, LENGTH = 256K
  RAM (xrw)  : ORIGIN = 0x20000000, LENGTH = 32K
}
_stack_start = ORIGIN(RAM) + LENGTH(RAM);
