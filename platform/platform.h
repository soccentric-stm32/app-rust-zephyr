/*
 * platform.h - C platform layer called from the Rust application
 *
 * Author:  Sandesh Ghimire <sandesh@soccentric.com>
 * Date:    2026-11-04
 * Target:  NUCLEO-F767ZI (Zephyr board nucleo_f767zi)
 * SRS:     RZCON-004, RZBT-002, RZBT-006, RZFN-003, RZRT-004, RZGEN-004
 *
 * Zephyr's Rust module binds the kernel, GPIO and flash APIs.  The UART,
 * watchdog, devicetree partition and fatal-error paths of this application
 * are not covered yet, so this layer wraps them in plain functions with
 * fixed arguments.  src/platform/plat.rs declares the same functions for Rust; keep
 * the two files in step.  No application logic lives here.
 */

#ifndef RZDEMO_PLATFORM_H
#define RZDEMO_PLATFORM_H

#include <stdbool.h>
#include <stdint.h>

#define PLAT_FAULT_TASK_LEN 16U
#define PLAT_FAULT_FILE_LEN 32U

/* Copy of the no-init fault record (same layout as the C demo's). */
struct plat_fault {
	uint32_t reason;
	uint32_t cfsr;
	uint32_t hfsr;
	uint32_t mmfar;
	uint32_t bfar;
	uint32_t pc;
	uint32_t lr;
	uint32_t psr;
	uint32_t line;
	char task[PLAT_FAULT_TASK_LEN];
	char file[PLAT_FAULT_FILE_LEN];
};

/* RZDBG-001: one debug line on USART6; level 'E', 'W', 'I' or 'D'.  The
 * text is copied (at most PLAT_LOG_TEXT_MAX bytes), not NUL-terminated. */
#define PLAT_LOG_TEXT_MAX 120U
void plat_log(uint8_t level, const uint8_t *text, uint32_t len);

/* Fault record (RZGEN-004). */
void plat_fault_boot_start(void);
bool plat_fault_get(struct plat_fault *out);
bool plat_fault_is_current_boot(void);
bool plat_fault_is_previous_boot(void);
void plat_fault_clear(void);
void plat_fatal(void);

/* System. */
uint32_t plat_vtor(void);
void plat_reboot(void);
void plat_dcache_invalidate(uintptr_t addr, uint32_t len);
void plat_thread_name(const char *name);
uint32_t plat_stack_unused(void);

/* IWDG through the watchdog API (RZRT-004). */
bool plat_wdt_init(void);
void plat_wdt_feed(void);

/* LD1..LD3 and B1 from the board devicetree aliases. */
bool plat_gpio_init(void);
void plat_led_set(uint32_t led, bool on);
bool plat_button_pressed(void);
void plat_button_wait(uint32_t timeout_ms);

/* USART3 HWLINK transport (RZFN-003). */
bool plat_uart_init(void);
bool plat_uart_set_baud(uint32_t baud);
uint32_t plat_uart_read(uint8_t *dst, uint32_t max_len);
void plat_uart_wait(uint32_t timeout_ms);
bool plat_uart_rx_running(void);
bool plat_uart_send(const uint8_t *data, uint32_t len, uint32_t timeout_ms);

/* The 32-byte CONFIRMED record in the running slot's metadata sector
 * (RZBT-006).  The offset is fixed here, never passed in. */
bool plat_confirmed_read(uint8_t *out32);
bool plat_confirmed_write(const uint8_t *record32);

#endif /* RZDEMO_PLATFORM_H */
