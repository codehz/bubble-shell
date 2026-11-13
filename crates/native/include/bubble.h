#pragma once
#include <stdint.h>

/* One shell per call to bubble_create. The creating thread is bound to it.
 * bubble_bind selects another shell for this thread. The host UI thread calls
 * everything except the measure callback. Measure runs on the UI thread during
 * bubble_frame and must not re-enter this ABI. Script work, including a wedged
 * loop, runs on that shell's private thread and does not hold the tree lock. */

typedef struct BubbleHost {
    void (*measure)(void *ctx, const uint8_t *text, uint32_t text_len, float font_size,
                    float line_height, uint16_t weight, float max_width, int has_max,
                    float *out_w, float *out_h);
    void *ctx;
    /* Bytes are a BLS1 layout snapshot. Return the byte count, or a larger count
     * when `cap` is too small, or -1 on failure. Coordinates match the draw. */
    int (*layout)(void *ctx, const uint8_t *text, uint32_t text_len, float width, float font_size,
                  float line_height, uint16_t weight, uint8_t align, float density, uint64_t revision,
                  uint8_t *out, uint32_t cap);
} BubbleHost;

int bubble_create(const BubbleHost *host);
/* Bind `id` (from bubble_current after create) on this thread. 0 ok, -1 missing. */
int bubble_bind(int id);
int bubble_current(void);
/* Destroy the shell bound on this thread. */
void bubble_destroy(void);

/* `entry` is the manifest entry file. Other manifest files are not accepted yet. */
int bubble_load(const char *manifest_json, const uint8_t *entry, uint32_t entry_len);

int bubble_frame(float width, float height, const uint8_t **paint, uint32_t *paint_len);
/* action: 0 down, 1 move, 2 up, 3 cancel; coordinates are logical units,
 * time_ms uses the same monotonic clock as bubble_tick. */
int bubble_pointer(uint32_t action, uint32_t id, float x, float y, double time_ms,
                   uint32_t pointer_kind, uint32_t buttons);
int bubble_tick(double time_ms);
/* Optional negotiation diagnostics; disabled by default. */
void bubble_set_gesture_trace(int enabled);
int bubble_take_gesture_trace(char *buf, uint32_t cap);
void bubble_scroll(float x, float y, float dx, float dy);
void bubble_set_visible(int visible);

/* action 0 focus a (0 blurs); 1 insert; 2 backspace; 3 delete; 4 selection a..b;
 * 5 composition start; 6 update; 7 end; 8 undo; 9 redo. Text is UTF-8. Offsets are UTF-16. */
int bubble_edit(uint32_t action, const uint8_t *text, uint32_t text_len, uint32_t a, uint32_t b);

/* Transactional editing. Every call takes the focused field's session id and
 * rejects stale ids. Offsets and cursor positions are UTF-16 units. */
int bubble_edit_snapshot(uint64_t *session, uint64_t *revision, uint32_t *anchor, uint32_t *active,
                         uint32_t *composition_start, uint32_t *composition_end,
                         uint32_t *composing);
int bubble_edit_commit(uint64_t session, const uint8_t *text, uint32_t text_len,
                       int32_t new_cursor_position);
int bubble_edit_compose(uint64_t session, const uint8_t *text, uint32_t text_len,
                        int32_t new_cursor_position);
int bubble_edit_replace(uint64_t session, uint32_t start, uint32_t end, const uint8_t *text,
                        uint32_t text_len, int32_t new_cursor_position, uint32_t composing);
int bubble_edit_delete_surrounding(uint64_t session, uint32_t before, uint32_t after);
int bubble_edit_delete_surrounding_code_points(uint64_t session, uint32_t before, uint32_t after);
int bubble_edit_set_composition(uint64_t session, uint32_t start, uint32_t end);
int bubble_edit_finish_composition(uint64_t session);
int bubble_edit_select(uint64_t session, uint32_t anchor, uint32_t active);
int bubble_edit_undo(uint64_t session);
int bubble_edit_redo(uint64_t session);
int bubble_edit_batch_begin(uint64_t session);
int bubble_edit_batch_end(uint64_t session);
int bubble_edit_batch_abort(uint64_t session);

/* Deepest text field under the point, or 0. */
uint32_t bubble_field_at(float x, float y);
uint32_t bubble_focus_at(float x, float y);
int bubble_caret(float *x, float *y, float *w, float *h);
uint32_t bubble_field_flags(uint32_t handle);
uint32_t bubble_focused(void);
/* Writes UTF-16 selection. mark_end is UINT32_MAX when no composition is open. */
int bubble_selection(uint32_t *start, uint32_t *end, uint32_t *mark_start, uint32_t *mark_end);
/* Nearest UTF-16 offset in the focused field. Does not move the caret. */
int bubble_unit_at(float x, float y, uint32_t *unit);
int bubble_caret_at(uint32_t unit, float *x, float *y, float *w, float *h);
uint32_t bubble_selection_count(void);
int bubble_selection_rect(uint32_t index, float *x, float *y, float *w, float *h);

void bubble_register_font(const char *family, const uint8_t *data, uint32_t len);
uint32_t bubble_font_epoch(void);
uint32_t bubble_font_count(void);

int bubble_node_text(uint32_t handle, char *buf, uint32_t cap);
int bubble_node_box(uint32_t handle, float *x, float *y, float *w, float *h);
float bubble_scroll_x_of(uint32_t handle);
float bubble_scroll_y_of(uint32_t handle);

/* A 1s interrupt and a memory limit fault both rebuild the context and
 * remount the loaded bundle. */
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
