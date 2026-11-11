#pragma once
#include <stdint.h>

/* Single-instance native shell. The host UI thread calls everything except
 * the measure callback. Measure runs on the UI thread during bubble_frame
 * and must not re-enter this ABI. Script work, including a wedged loop, runs
 * on a private thread and does not hold the tree lock. */

typedef struct BubbleHost {
    void (*measure)(void *ctx, const uint8_t *text, uint32_t text_len, float font_size,
                    float max_width, int has_max, float *out_w, float *out_h);
    void *ctx;
} BubbleHost;

int bubble_create(const BubbleHost *host);
void bubble_destroy(void);

/* `entry` is the manifest entry file. Other manifest files are not accepted yet. */
int bubble_load(const char *manifest_json, const uint8_t *entry, uint32_t entry_len);

int bubble_frame(float width, float height, const uint8_t **paint, uint32_t *paint_len);
void bubble_pointer_down(float x, float y);
void bubble_scroll(float x, float y, float dy);
void bubble_set_visible(int visible);

void bubble_register_font(const char *family, const uint8_t *data, uint32_t len);
uint32_t bubble_font_epoch(void);
uint32_t bubble_font_count(void);

int bubble_node_text(uint32_t handle, char *buf, uint32_t cap);
int bubble_node_box(uint32_t handle, float *x, float *y, float *w, float *h);
float bubble_scroll_of(uint32_t handle);

/* Blocks the caller until the script thread finishes, including a 1s interrupt. */
int bubble_eval(const char *source, char *buf, uint32_t cap);
/* Returns immediately. A dead loop is interrupted off the caller thread. */
void bubble_post_eval(const char *source);

uint32_t bubble_generation(void);
uint32_t bubble_faults(void);
int bubble_status(char *buf, uint32_t cap);
const char *bubble_last_error(void);

int bubble_poll_request(uint32_t *id, char *method, uint32_t method_cap, char *params,
                        uint32_t params_cap);
void bubble_finish_request(uint32_t id, int ok, const char *result);
