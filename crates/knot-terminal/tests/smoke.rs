//! Checks that the linked libghostty-vt parses styled output and reads the
//! screen back through the raw bindings.

use std::ptr;

use knot_terminal::ffi::*;

#[test]
fn styled_output_reads_back() {
    unsafe {
        let mut terminal: GhosttyTerminal = ptr::null_mut();
        assert_eq!(
            ghostty_terminal_new(ptr::null(), &mut terminal, 20, 3),
            GHOSTTY_SUCCESS
        );

        let output = b"\x1b[1;31mhello\x1b[0m world\r\nsecond";
        ghostty_terminal_vt_write(terminal, output.as_ptr(), output.len());

        let mut formatter: GhosttyFormatter = ptr::null_mut();
        assert_eq!(
            ghostty_formatter_terminal_new(ptr::null(), &mut formatter, terminal),
            GHOSTTY_SUCCESS
        );
        let mut text = [0u8; 256];
        let mut written = 0;
        assert_eq!(
            ghostty_formatter_format_buf(formatter, text.as_mut_ptr(), text.len(), &mut written),
            GHOSTTY_SUCCESS
        );
        ghostty_formatter_free(formatter);
        assert_eq!(&text[..written], b"hello world\nsecond");

        let style_at = |x| {
            let point = GhosttyPoint {
                tag: GHOSTTY_POINT_TAG_ACTIVE,
                value: GhosttyPointValue {
                    coordinate: GhosttyPointCoordinate { x, y: 0 },
                },
            };
            let mut grid_ref = std::mem::zeroed::<GhosttyGridRef>();
            assert_eq!(
                ghostty_terminal_grid_ref(terminal, point, &mut grid_ref),
                GHOSTTY_SUCCESS
            );
            let mut style = std::mem::zeroed::<GhosttyStyle>();
            assert_eq!(
                ghostty_grid_ref_style(&grid_ref, &mut style),
                GHOSTTY_SUCCESS
            );
            style
        };
        let hello = style_at(0);
        assert_ne!(u32::from(hello.flags) & GHOSTTY_STYLE_FLAG_BOLD, 0);
        assert_eq!(i32::from(hello.fg_color.tag), GHOSTTY_STYLE_COLOR_PALETTE);
        assert_eq!(hello.fg_color.value.palette, 1);
        assert!(ghostty_style_is_default(&style_at(6)));

        ghostty_terminal_free(terminal);
    }
}
