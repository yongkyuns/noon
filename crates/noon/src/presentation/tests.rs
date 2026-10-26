use super::*;

#[test]
fn preset_values_are_finite_and_themes_remain_independent() {
    let dark_title = Theme::dark().text_style(TextRole::Title);
    for theme in [Theme::dark(), Theme::light()] {
        for role in [
            ColorRole::Background,
            ColorRole::Surface,
            ColorRole::RaisedSurface,
            ColorRole::Foreground,
            ColorRole::Muted,
            ColorRole::Outline,
            ColorRole::Accent,
            ColorRole::Positive,
            ColorRole::Warning,
            ColorRole::Negative,
        ] {
            let color = theme.color(role);
            for channel in [color.red, color.green, color.blue, color.alpha] {
                assert!(channel.is_finite() && (0.0..=1.0).contains(&channel));
            }
        }
        for role in [
            TextRole::Title,
            TextRole::Subtitle,
            TextRole::Body,
            TextRole::Caption,
            TextRole::Label,
            TextRole::Code,
        ] {
            let style = theme.text_style(role);
            assert!(!style.font_family().is_empty());
            assert!(style.font_size().is_finite() && style.font_size() > 0.0);
            assert!(style.line_spacing().is_finite());
        }
        for role in [
            SpacingRole::Page,
            SpacingRole::Panel,
            SpacingRole::Small,
            SpacingRole::Normal,
            SpacingRole::Large,
        ] {
            assert!(theme.spacing(role).is_finite() && theme.spacing(role) > 0.0);
        }
        assert!(theme.spacing(SpacingRole::Small) < theme.spacing(SpacingRole::Normal));
        assert!(theme.spacing(SpacingRole::Normal) < theme.spacing(SpacingRole::Large));
    }
    assert_eq!(dark_title, Theme::dark().text_style(TextRole::Title));
    assert_ne!(
        dark_title.color(),
        Theme::light().text_style(TextRole::Title).color()
    );
    assert_eq!(
        dark_title.font_size(),
        Theme::light().text_style(TextRole::Title).font_size()
    );
}

#[test]
fn categorical_lookup_rejects_exhausted_palettes_without_cycling() {
    for theme in [Theme::dark(), Theme::light()] {
        let count = theme.categorical_count();
        assert!(count > 1);
        let colors: Vec<_> = (0..count)
            .map(|index| theme.categorical(index).unwrap())
            .collect();
        for (index, color) in colors.iter().enumerate() {
            assert!(!colors[..index].contains(color));
        }
        for index in [count, usize::MAX] {
            assert_eq!(
                theme.categorical(index),
                Err(PresentationError::CategoricalIndexOutOfRange { index, count })
            );
        }
    }
}

#[test]
fn language_boundary_names_are_checked_in_shared_rust() {
    assert_eq!("light".parse::<Theme>().unwrap(), Theme::light());
    assert_eq!("dark".parse::<Theme>().unwrap(), Theme::dark());
    assert_eq!(
        "raised_surface".parse::<ColorRole>().unwrap(),
        ColorRole::RaisedSurface
    );
    assert_eq!("caption".parse::<TextRole>().unwrap(), TextRole::Caption);
    assert_eq!("panel".parse::<SpacingRole>().unwrap(), SpacingRole::Panel);
    assert!("Dark".parse::<Theme>().is_err());
    assert!("warning".parse::<TextRole>().is_err());
    assert!("caption".parse::<ColorRole>().is_err());
    assert!("".parse::<SpacingRole>().is_err());
}

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
#[test]
fn themed_text_uses_ordinary_admission_and_explicit_overrides() {
    use crate::{Scene, SemanticPaint, Text};

    let dark = Theme::dark();
    let light = Theme::light();
    let original = dark.text("A title", TextRole::Title);
    let override_color = dark.color(ColorRole::Warning);
    let overridden = dark
        .text("A title", TextRole::Title)
        .with_font_size(31.0)
        .with_line_spacing(0.25)
        .color(override_color);
    assert_eq!(
        original.font_size(),
        dark.text_style(TextRole::Title).font_size()
    );
    assert_eq!(overridden.font_size(), 31.0);
    assert_eq!(overridden.line_spacing(), 0.25);

    let mut scene = Scene::new();
    let title = scene.text(original).unwrap();
    let changed = scene.text(overridden).unwrap();
    let light_title = scene.text(light.text("A title", TextRole::Title)).unwrap();
    let unthemed = scene.text(Text::new("Unchanged defaults")).unwrap();
    for (object, expected) in [
        (&title, dark.color(ColorRole::Foreground)),
        (&changed, override_color),
        (&light_title, light.color(ColorRole::Foreground)),
        (&unthemed, Color::WHITE),
    ] {
        assert_eq!(
            scene.authored(object).unwrap().style.fill,
            Some(SemanticPaint::Solid(expected))
        );
        assert!(scene.authored(object).unwrap().content.text().is_some());
        scene.add(object).unwrap();
    }
    let session = scene.execution_session().unwrap();
    assert_eq!(session.frame().objects.len(), 4);
    assert_eq!(
        dark.text("Later", TextRole::Title).font_size(),
        dark.text_style(TextRole::Title).font_size()
    );
}

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
#[test]
fn paired_example_uses_the_normal_execution_session() {
    let session = crate::example_scenes::presentation_presets::session().unwrap();
    assert_eq!(session.frame().objects.len(), 26);
}
