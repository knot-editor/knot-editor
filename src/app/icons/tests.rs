use gpui::{Context, TestAppContext, px};

use super::*;
use crate::app::settings::SettingsScope;
use knot_model::icons::NATIVE_ICONS;

const RED_SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#ff0000"/></svg>"##;

#[test]
fn every_native_default_names_an_existing_lucide_glyph() {
    for (name, glyph) in NATIVE_ICONS {
        assert!(
            AllAssets::get(&format!("icons/{glyph}.svg")).is_some(),
            "{name} names the missing Lucide glyph {glyph}"
        );
    }
}

const IDS: [&str; 4] = ["icon-0", "icon-1", "icon-2", "icon-3"];

struct Icons(Vec<IconDefinition>);

impl Render for Icons {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_start()
            .children(
                self.0
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(index, definition)| {
                        div()
                            .debug_selector(|| IDS[index].to_owned())
                            .text_size(px(20.))
                            .child(AppIcon::new(definition))
                    }),
            )
    }
}

#[gpui::test]
fn tinted_and_full_color_icons_paint_at_the_text_size(cx: &mut TestAppContext) {
    let definitions = vec![
        IconDefinition::lucide("folder-open"),
        IconDefinition {
            source: IconSource::Svg(RED_SQUARE.into()),
            color: IconColor::Key(color::ERROR.into()),
        },
        IconDefinition {
            source: IconSource::Svg(RED_SQUARE.into()),
            color: IconColor::Original,
        },
        IconDefinition::lucide("no-such-glyph"),
    ];
    let (_, cx) = cx.add_window_view(|_, _| Icons(definitions));
    cx.run_until_parked();
    for (index, id) in IDS.into_iter().enumerate() {
        let bounds = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("icon {index} was laid out"));
        assert_eq!(bounds.size.width, px(20.), "icon {index}");
        assert_eq!(bounds.size.height, px(20.), "icon {index}");
    }
}

#[gpui::test]
fn the_pack_setting_selects_the_pack_and_follows_changes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(ApplicationSettings::default());
        let owner = knot_model::icons::IconOwner {
            extension: knot_protocol::ExtensionId::new(1),
            lifecycle: knot_protocol::ExtensionLifecycleId::new(1),
        };
        let svg = IconDefinition {
            source: IconSource::Svg(RED_SQUARE.into()),
            color: IconColor::Original,
        };
        cx.default_global::<ApplicationIcons>().0.register_pack(
            owner,
            "Red",
            [("file".to_owned(), svg)].into(),
        );
        apply_at_startup(cx);
        assert_eq!(
            file_definition(None, None, cx),
            IconDefinition::lucide("file")
        );
    });
    cx.update(|cx| {
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(ICONS_PACK, "Red".into(), SettingsScope::default())
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(file_definition(None, None, cx).color, IconColor::Original);
        assert_eq!(
            folder_definition(None, false, cx),
            IconDefinition::lucide("folder")
        );
        cx.global_mut::<ApplicationSettings>()
            .0
            .set(ICONS_PACK, "".into(), SettingsScope::default())
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            file_definition(None, None, cx),
            IconDefinition::lucide("file")
        )
    });
}
