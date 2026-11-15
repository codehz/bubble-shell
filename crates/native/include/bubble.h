#pragma once
#include <stdint.h>

typedef struct BubbleShell BubbleShell;

/* One shell per call to bubble_create. Pass the returned pointer to every
 * other call. Do not destroy a shell while another call on it is running.
 * The host UI thread calls everything except the measure callback. Measure runs
 * on the UI thread during bubble_frame and must not re-enter this ABI. Script
 * work, including a wedged loop, runs on that shell's private thread and does
 * not hold the tree lock. */

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

BubbleShell *bubble_create(const BubbleHost *host);
/* Free an error string from bubble_load, bubble_set_params,
 * bubble_set_storage_dir, or bubble_send. Null is a no-op. */
void bubble_string_free(char *s);
/* Destroy shell. Null is a no-op. The pointer is invalid afterwards. */
void bubble_destroy(BubbleShell *shell);
/* JSON object or array. Applied on the next bubble_load of the bound shell. */
char *bubble_set_params(BubbleShell *shell, const char *json);
/* Directory for bubble.storage.<manifest id>.json. Null keeps storage in memory. */
char *bubble_set_storage_dir(BubbleShell *shell, const char *path);
/* JSON payload for host.on(name). Queued until a listener exists. */
char *bubble_send(BubbleShell *shell, const char *name, const char *payload);
/* 1 when the loaded bundle registered bubbleHost.onBack, else 0. */
int bubble_has_back_handler(BubbleShell *shell);

/* `entry` is the manifest entry file. Other manifest files are not accepted yet. */
char *bubble_load(BubbleShell *shell, const char *manifest_json, const uint8_t *entry, uint32_t entry_len);

int bubble_frame(BubbleShell *shell, float width, float height, const uint8_t **paint, uint32_t *paint_len);
/* action: 0 down, 1 move, 2 up, 3 cancel; coordinates are logical units,
 * time_ms uses the same monotonic clock as bubble_tick. */
int bubble_pointer(BubbleShell *shell, uint32_t action, uint32_t id, float x, float y, double time_ms,
                   uint32_t pointer_kind, uint32_t buttons);
/* Pressed feedback only: no recognition, no tap, no scroll. Hosts whose own
 * pointer path owns a sample deliver it here instead of bubble_pointer. */
int bubble_visual_pointer(BubbleShell *shell, uint32_t action, uint32_t id, float x, float y, double time_ms,
                          uint32_t pointer_kind, uint32_t buttons);
int bubble_tick(BubbleShell *shell, double time_ms);
/* Optional negotiation diagnostics; disabled by default. */
void bubble_set_gesture_trace(BubbleShell *shell, int enabled);
int bubble_take_gesture_trace(BubbleShell *shell, char *buf, uint32_t cap);
void bubble_scroll(BubbleShell *shell, float x, float y, float dx, float dy);
void bubble_set_visible(BubbleShell *shell, int visible);

/* action 0 focus a (0 blurs); 1 insert; 2 backspace; 3 delete; 4 selection a..b;
 * 5 composition start; 6 update; 7 end; 8 undo; 9 redo. Text is UTF-8. Offsets are UTF-16. */
int bubble_edit(BubbleShell *shell, uint32_t action, const uint8_t *text, uint32_t text_len, uint32_t a, uint32_t b);

/* Transactional editing. Every call takes the focused field's session id and
 * rejects stale ids. Offsets and cursor positions are UTF-16 units. */
int bubble_edit_snapshot(BubbleShell *shell, uint64_t *session, uint64_t *revision, uint32_t *anchor, uint32_t *active,
                         uint32_t *composition_start, uint32_t *composition_end,
                         uint32_t *composing);
int bubble_edit_commit(BubbleShell *shell, uint64_t session, const uint8_t *text, uint32_t text_len,
                       int32_t new_cursor_position);
int bubble_edit_compose(BubbleShell *shell, uint64_t session, const uint8_t *text, uint32_t text_len,
                        int32_t new_cursor_position);
int bubble_edit_replace(BubbleShell *shell, uint64_t session, uint32_t start, uint32_t end, const uint8_t *text,
                        uint32_t text_len, int32_t new_cursor_position, uint32_t composing);
int bubble_edit_delete_surrounding(BubbleShell *shell, uint64_t session, uint32_t before, uint32_t after);
int bubble_edit_delete_surrounding_code_points(BubbleShell *shell, uint64_t session, uint32_t before, uint32_t after);
int bubble_edit_set_composition(BubbleShell *shell, uint64_t session, uint32_t start, uint32_t end);
int bubble_edit_finish_composition(BubbleShell *shell, uint64_t session);
int bubble_edit_select(BubbleShell *shell, uint64_t session, uint32_t anchor, uint32_t active);
int bubble_edit_undo(BubbleShell *shell, uint64_t session);
int bubble_edit_redo(BubbleShell *shell, uint64_t session);
int bubble_edit_batch_begin(BubbleShell *shell, uint64_t session);
int bubble_edit_batch_end(BubbleShell *shell, uint64_t session);
int bubble_edit_batch_abort(BubbleShell *shell, uint64_t session);

/* Deepest text field under the point, or 0. */
uint32_t bubble_field_at(BubbleShell *shell, float x, float y);
uint32_t bubble_focus_at(BubbleShell *shell, float x, float y);
int bubble_caret(BubbleShell *shell, float *x, float *y, float *w, float *h);
uint32_t bubble_field_flags(BubbleShell *shell, uint32_t handle);
uint32_t bubble_focused(BubbleShell *shell);
/* Writes UTF-16 selection. mark_end is UINT32_MAX when no composition is open. */
int bubble_selection(BubbleShell *shell, uint32_t *start, uint32_t *end, uint32_t *mark_start, uint32_t *mark_end);
/* Nearest UTF-16 offset in the focused field. Does not move the caret. */
int bubble_unit_at(BubbleShell *shell, float x, float y, uint32_t *unit);
int bubble_caret_at(BubbleShell *shell, uint32_t unit, float *x, float *y, float *w, float *h);
uint32_t bubble_selection_count(BubbleShell *shell);
int bubble_selection_rect(BubbleShell *shell, uint32_t index, float *x, float *y, float *w, float *h);

void bubble_register_font(BubbleShell *shell, const char *family, const uint8_t *data, uint32_t len);
uint32_t bubble_font_epoch(BubbleShell *shell);
uint32_t bubble_font_count(BubbleShell *shell);

int bubble_node_text(BubbleShell *shell, uint32_t handle, char *buf, uint32_t cap);
int bubble_node_box(BubbleShell *shell, uint32_t handle, float *x, float *y, float *w, float *h);
float bubble_scroll_x_of(BubbleShell *shell, uint32_t handle);
float bubble_scroll_y_of(BubbleShell *shell, uint32_t handle);

/* A 1s interrupt and a memory limit fault both rebuild the context and
 * remount the loaded bundle. */
/* Blocks the caller until the script thread finishes, including a 1s interrupt. */
int bubble_eval(BubbleShell *shell, const char *source, char *buf, uint32_t cap);
/* Returns immediately. A dead loop is interrupted off the caller thread. */
void bubble_post_eval(BubbleShell *shell, const char *source);

uint32_t bubble_generation(BubbleShell *shell);
uint32_t bubble_faults(BubbleShell *shell);
int bubble_status(BubbleShell *shell, char *buf, uint32_t cap);
/* Borrowed. Invalid until the next fallible call on shell, or bubble_destroy. */
const char *bubble_last_error(BubbleShell *shell);

int bubble_poll_request(BubbleShell *shell, uint32_t *id, char *method, uint32_t method_cap, char *params,
                        uint32_t params_cap);
void bubble_finish_request(BubbleShell *shell, uint32_t id, int ok, const char *result);
