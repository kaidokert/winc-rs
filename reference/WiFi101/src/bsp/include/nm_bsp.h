// <license>
/** \defgroup nm_bsp BSP
 */
/**@defgroup  BSPDefine Defines
 * @ingroup nm_bsp
 * @{
 */
#ifndef _NM_BSP_H_
#define _NM_BSP_H_

#define NMI_API
/*!<
 *  Attribute used to define the memory section to map Functions in host memory.
*/
#define CONST const

/*!<
*     Used for code portability.
*/

/*!
 * @typedef      void (*tpfNmBspIsr) (void);
 * @brief           Pointer to function.\n
 *                     Used as a data type of ISR function registered by \ref nm_bsp_register_isr
 * @return         None
 */
typedef void (*tpfNmBspIsr)(void);



#ifndef NULL
#define NULL ((void*)0)
#endif
/*!<
*   Void Pointer to '0' in case NULL is not defined.
*/

#define BSP_MIN(x,y) ((x)>(y)?(y):(x))
/*!<
*     Computes the minimum value between \b x and \b y.
*/

 //@}

/**@defgroup  DataT  DataTypes
 * @ingroup nm_bsp
 * @{
 */

  /*!
 * @ingroup DataTypes
 * @typedef      unsigned char	uint8;
 * @brief        Range of values between 0 to 255
 */
typedef unsigned char	uint8;

 /*!
 * @ingroup DataTypes
 * @typedef      unsigned short	uint16;
 * @brief        Range of values between 0 to 65535
 */
typedef unsigned short	uint16;

 /*!
 * @ingroup Data Types
 * @typedef      unsigned long	uint32;
 * @brief        Range of values between 0 to 4294967295
 */
typedef unsigned long	uint32;



  /*!
 * @ingroup Data Types
 * @typedef      signed char		sint8;
 * @brief        Range of values between -128 to 127
 */
typedef signed char		sint8;

 /*!
 * @ingroup DataTypes
 * @typedef      signed short	sint16;
 * @brief        Range of values between -32768 to 32767
 */
typedef signed short	sint16;

  /*!
 * @ingroup DataTypes
 * @typedef      signed long		sint32;
 * @brief        Range of values between -2147483648 to 2147483647
 */

typedef signed long		sint32;
 //@}

#ifndef CORTUS_APP

#ifdef __cplusplus
extern "C"{
#endif

/** \defgroup BSPAPI Function
 *   @ingroup nm_bsp
 */


/** @defgroup NmBspInitFn nm_bsp_init
 *  @ingroup BSPAPI
 *  Initialization for BSP such as Reset and Chip Enable Pins for WINC, delays, register ISR, enable/disable IRQ for WINC, ...etc. You must use this function in the head of your application to
 *  enable WINC and Host Driver communicate each other.
 */
 /**@{*/
/*!
 * @fn           sint8 nm_bsp_init(void);
 * @note         Implementation of this function is host dependent.
 * @warning      Missing use will lead to unavailability of host communication.\n
 *
 * @return       The function returns @ref M2M_SUCCESS for successful operations and a negative value otherwise.

 */
sint8 nm_bsp_init(void);
 /**@}*/


 /** @defgroup NmBspDeinitFn nm_bsp_deinit
 *    @ingroup BSPAPI
 *   	 De-initialization for BSP (\e Board \e Support \e Package)
 */
 /**@{*/
/*!
 * @fn           sint8 nm_bsp_deinit(void);
 * @pre          Initialize \ref nm_bsp_init first
 * @note         Implementation of this function is host dependent.
 * @warning      Missing use may lead to unknown behavior in case of soft reset.\n
 * @see          nm_bsp_init
 * @return      The function returns @ref M2M_SUCCESS for successful operations and a negative value otherwise.

 */
sint8 nm_bsp_deinit(void);
 /**@}*/


/** @defgroup NmBspResetFn  nm_bsp_reset
*     @ingroup BSPAPI
*      Resetting NMC1500 SoC by setting CHIP_EN and RESET_N signals low, then after specific delay the function will put CHIP_EN high then RESET_N high,
*      for the timing between signals please review the WINC data-sheet
*/
/**@{*/
 /*!
 * @fn           void nm_bsp_reset(void);
 * @param [in]   None
 * @pre          Initialize \ref nm_bsp_init first
 * @note         Implementation of this function is host dependent and called by HIF layer.
 * @see          nm_bsp_init
 * @return       None

 */
void nm_bsp_reset(void);
 /**@}*/


/** @defgroup NmBspSleepFn nm_bsp_sleep
*     @ingroup BSPAPI
*     Sleep in units of milliseconds.\n
*    This function used by HIF Layer according to different situations.
*/
/**@{*/
/*!
 * @fn           void nm_bsp_sleep(uint32);
 * @brief
 * @param [in]   u32TimeMsec
 *               Time unit in milliseconds
 * @pre          Initialize \ref nm_bsp_init first
 * @warning      Maximum value must nor exceed 4294967295 milliseconds which is equal to 4294967.295 seconds.\n
 * @note         Implementation of this function is host dependent.
 * @see           nm_bsp_init
 * @return       None
 */
void nm_bsp_sleep(uint32 u32TimeMsec);
/**@}*/


/** @defgroup NmBspRegisterFn nm_bsp_register_isr
*     @ingroup BSPAPI
*   Register ISR (Interrupt Service Routine) in the initialization of HIF (Host Interface) Layer.
*   When the interrupt trigger the BSP layer should call the pfisr function once inside the interrupt.
*/
/**@{*/
/*!
 * @fn           void nm_bsp_register_isr(tpfNmBspIsr);
 * @param [in]   tpfNmBspIsr  pfIsr
 *               Pointer to ISR handler in HIF
 * @warning      Make sure that ISR for IRQ pin for WINC is disabled by default in your implementation.
 * @see          tpfNmBspIsr
 * @return       None
 */
void nm_bsp_register_isr(tpfNmBspIsr pfIsr);
/**@}*/


/** @defgroup NmBspInterruptCtrl nm_bsp_interrupt_ctrl
*     @ingroup BSPAPI
 *      Synchronous enable/disable of WINC to host interrupts.
 *  @{
*/
/*!
 * @fn          void nm_bsp_interrupt_ctrl(uint8 u8Enable);
 * @brief       Enable/Disable interrupts from the WINC.
 * @details     This function can be used to enable/disable the WINC to host interrupts, depending on how
 *              the driver is implemented. It is an internal driver function and shouldn't be called by
 *              the application.
 * @param [in]   u8Enable
 *                  - '0' disable interrupts.
 *                  - '1' enable interrupts.
 * @pre         The interrupt must be registered using @ref nm_bsp_register_isr first.
 * @note         Implementation of this function is host dependent and called by HIF layer.
 * @see         tpfNmBspIsr, nm_bsp_register_isr
 * @return       None
 */
void nm_bsp_interrupt_ctrl(uint8 u8Enable);
  /**@}*/

#ifdef __cplusplus
}
#endif

#endif

/**
 * @addtogroup BSPDefine
 * @{
 */
#ifdef _NM_BSP_BIG_END
/*! Switch endianness of 32bit word (In the case that Host is BE) */
#define NM_BSP_B_L_32(x) \
((((x) & 0x000000FF) << 24) + \
(((x) & 0x0000FF00) << 8)  + \
(((x) & 0x00FF0000) >> 8)   + \
(((x) & 0xFF000000) >> 24))

/*! Switch endianness of 16bit word (In the case that Host is BE) */
#define NM_BSP_B_L_16(x) \
((((x) & 0x00FF) << 8) + \
(((x)  & 0xFF00) >> 8))
#else
/*! Retain endianness of 32bit word (In the case that Host is LE) */
#define NM_BSP_B_L_32(x)  (x)
/*! Retain endianness of 16bit word (In the case that Host is LE) */
#define NM_BSP_B_L_16(x)  (x)
#endif
/**@}*/     //BSPDefine

#endif	/*_NM_BSP_H_*/
