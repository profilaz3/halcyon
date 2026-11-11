//! A window-local key recorder. This does not grab keys from COSMIC.
//! The wrapper intercepts keys before inputs/buttons see them, while the
//! application suspends its own navigation shortcuts. Mouse controls and
//! layout are forwarded unchanged, including when recording is inactive.
use super::Message as Edit;
use crate::desktop::shortcuts::{Binding, format::Modifiers};
use crate::settings::Message;
use cosmic::iced::advanced::widget::{Operation, Tree};
use cosmic::iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector, keyboard, mouse, window};
use cosmic::{Element, Renderer, Theme};
use xkbcommon::xkb::{self, Keysym};

fn combination(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    location: keyboard::Location,
) -> Result<String, String> {
    use keyboard::Key;
    use keyboard::key::Named;
    let name = match key.as_ref() {
        Key::Character(text) => {
            let mut chars = text.chars();
            let Some(character) = chars.next().filter(|_| chars.next().is_none()) else {
                return Err("This key cannot be recorded. Type its combination instead.".into());
            };
            if location == keyboard::Location::Numpad {
                match character {
                    '0'..='9' => format!("KP_{character}"),
                    '+' => "KP_Add".into(),
                    '-' => "KP_Subtract".into(),
                    '*' => "KP_Multiply".into(),
                    '/' => "KP_Divide".into(),
                    '.' => "KP_Decimal".into(),
                    ',' => "KP_Separator".into(),
                    '=' => "KP_Equal".into(),
                    _ => xkb::keysym_get_name(Keysym::from_char(character)),
                }
            } else {
                let character = character.to_lowercase().next().unwrap_or(character);
                xkb::keysym_get_name(Keysym::from_char(character))
            }
        }
        Key::Named(named) => {
            let name = match named {
                Named::Enter => "Return",
                Named::ArrowLeft => "Left",
                Named::ArrowRight => "Right",
                Named::ArrowUp => "Up",
                Named::ArrowDown => "Down",
                Named::PageUp => "Page_Up",
                Named::PageDown => "Page_Down",
                Named::Backspace => "BackSpace",
                Named::ContextMenu => "Menu",
                Named::PrintScreen => "Print",
                Named::MediaPlay | Named::MediaPlayPause => "XF86AudioPlay",
                Named::MediaPause => "XF86AudioPause",
                Named::MediaStop => "XF86AudioStop",
                Named::MediaTrackNext => "XF86AudioNext",
                Named::MediaTrackPrevious => "XF86AudioPrev",
                Named::MediaRewind => "XF86AudioRewind",
                Named::MediaFastForward => "XF86AudioForward",
                Named::AudioVolumeUp => "XF86AudioRaiseVolume",
                Named::AudioVolumeDown => "XF86AudioLowerVolume",
                Named::AudioVolumeMute => "XF86AudioMute",
                Named::MicrophoneVolumeMute => "XF86AudioMicMute",
                Named::BrightnessUp => "XF86MonBrightnessUp",
                Named::BrightnessDown => "XF86MonBrightnessDown",
                _ => "",
            };
            let name = if name.is_empty() {
                format!("{named:?}")
            } else {
                name.into()
            };
            if location == keyboard::Location::Numpad {
                match named {
                    Named::Enter => "KP_Enter".into(),
                    Named::ArrowLeft
                    | Named::ArrowRight
                    | Named::ArrowUp
                    | Named::ArrowDown
                    | Named::PageUp
                    | Named::PageDown
                    | Named::Home
                    | Named::End
                    | Named::Insert
                    | Named::Delete => format!("KP_{name}"),
                    _ => name,
                }
            } else {
                name
            }
        }
        Key::Unidentified => {
            return Err("This key cannot be recorded. Type its combination instead.".into());
        }
    };
    let sym = xkb::keysym_from_name(&name, xkb::KEYSYM_CASE_INSENSITIVE);
    if sym.raw() == 0 {
        return Err("This key cannot be recorded. Type its combination instead.".into());
    }
    let binding = Binding::new(
        Modifiers {
            ctrl: modifiers.control(),
            alt: modifiers.alt(),
            shift: modifiers.shift(),
            logo: modifiers.logo(),
        },
        Some(sym),
    );
    // The typed and recorded paths deliberately use the same validator.
    Binding::parse_new(&binding.to_string()).map(|binding| binding.to_string())
}

fn input(event: &Event) -> Option<Edit> {
    use keyboard::key::Named;
    match event {
        Event::Window(window::Event::Unfocused) => Some(Edit::CancelRecording),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            location,
            repeat: false,
            ..
        }) => match key {
            keyboard::Key::Named(Named::Escape) => Some(Edit::CancelRecording),
            keyboard::Key::Named(
                Named::Shift
                | Named::Control
                | Named::Alt
                | Named::AltGraph
                | Named::Super
                | Named::Meta
                | Named::Hyper
                | Named::Fn
                | Named::FnLock,
            ) => None,
            _ => Some(Edit::Recorded(combination(key, *modifiers, *location))),
        },
        _ => None,
    }
}

pub fn guard<'a>(content: Element<'a, Message>, active: bool) -> Element<'a, Message> {
    Element::new(Recorder { content, active })
}

struct Recorder<'a> {
    content: Element<'a, Message>,
    active: bool,
}

impl Widget<Message, Theme, Renderer> for Recorder<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn a11y_nodes(
        &self,
        layout: Layout<'_>,
        tree: &Tree,
        cursor: mouse::Cursor,
    ) -> iced_accessibility::A11yTree {
        self.content
            .as_widget()
            .a11y_nodes(layout, &tree.children[0], cursor)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if self.active {
            if let Some(message) = input(event) {
                shell.publish(Message::Shortcuts(message));
            }
            if matches!(event, Event::Keyboard(_)) {
                shell.capture_event();
                return;
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn drag_destinations(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        rectangles: &mut cosmic::iced::advanced::clipboard::DndDestinationRectangles,
    ) {
        self.content
            .as_widget()
            .drag_destinations(&tree.children[0], layout, renderer, rectangles);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        // An open popup must not receive recorded keys before this wrapper.
        if self.active {
            None
        } else {
            self.content.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyboard::key::Named;
    use keyboard::{Key, Location, Modifiers};

    #[test]
    fn recorded_chords_match_typed_identity_and_preserve_keypad_keys() {
        for (key, modifiers, location, expected) in [
            (
                Key::Character("K".into()),
                Modifiers::CTRL | Modifiers::SHIFT,
                Location::Standard,
                "Ctrl+Shift+k",
            ),
            (
                Key::Character("+".into()),
                Modifiers::CTRL,
                Location::Standard,
                "Ctrl+plus",
            ),
            (
                Key::Character("7".into()),
                Modifiers::ALT,
                Location::Numpad,
                "Alt+KP_7",
            ),
            (
                Key::Named(Named::Enter),
                Modifiers::CTRL,
                Location::Numpad,
                "Ctrl+KP_Enter",
            ),
            (
                Key::Named(Named::ArrowLeft),
                Modifiers::ALT,
                Location::Standard,
                "Alt+Left",
            ),
            (
                Key::Named(Named::F7),
                Modifiers::CTRL,
                Location::Standard,
                "Ctrl+F7",
            ),
            (
                Key::Named(Named::AudioVolumeMute),
                Modifiers::empty(),
                Location::Standard,
                "XF86AudioMute",
            ),
        ] {
            let recorded = combination(&key, modifiers, location).unwrap();
            assert_eq!(
                Binding::parse_new(&recorded).unwrap().ident(),
                Binding::parse_new(expected).unwrap().ident(),
                "{recorded}"
            );
        }
        assert!(
            combination(
                &Key::Character("x".into()),
                Modifiers::empty(),
                Location::Standard
            )
            .is_err()
        );
        assert!(combination(&Key::Unidentified, Modifiers::CTRL, Location::Standard).is_err());
    }

    #[test]
    fn escape_focus_loss_and_modifier_presses_do_not_record_a_chord() {
        let press = |key| {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key: Key::Unidentified,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            })
        };
        assert_eq!(
            input(&press(Key::Named(Named::Escape))),
            Some(Edit::CancelRecording)
        );
        assert_eq!(
            input(&Event::Window(window::Event::Unfocused)),
            Some(Edit::CancelRecording)
        );
        assert_eq!(input(&press(Key::Named(Named::Control))), None);
        assert_eq!(input(&press(Key::Named(Named::Super))), None);
    }

    #[test]
    fn recording_intercepts_keys_before_the_focused_input() {
        use crate::native_test::Native;
        use cosmic::iced::{Point, Size, event::Status};
        let view = |active| {
            guard(
                cosmic::widget::text_input("Name", "Existing")
                    .id(cosmic::widget::Id::new("recording-input-test"))
                    .on_input(|text| Message::Shortcuts(Edit::Name(text)))
                    .into(),
                active,
            )
        };
        let mut native = Native::new(Size::new(500., 80.));
        let point = native.bounds(view(false), "recording-input-test").center();
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            native.event(view(false), Event::Mouse(event), point);
        }
        let press = |key: Key| {
            Event::Keyboard(keyboard::Event::KeyPressed {
                modified_key: key.clone(),
                key,
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyK),
                location: Location::Standard,
                modifiers: Modifiers::empty(),
                text: Some("k".into()),
                repeat: false,
            })
        };
        let (messages, status) = native.event(view(true), press(Key::Character("k".into())), point);
        assert_eq!(status, Status::Captured);
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            &messages[0],
            Message::Shortcuts(Edit::Recorded(Err(_)))
        ));
        let (messages, _) = native.event(view(true), press(Key::Named(Named::Escape)), point);
        assert!(matches!(
            messages.as_slice(),
            [Message::Shortcuts(Edit::CancelRecording)]
        ));
        let (messages, _) = native.event(
            view(false),
            press(Key::Character("k".into())),
            Point::new(1., 1.),
        );
        assert!(
            matches!(messages.as_slice(), [Message::Shortcuts(Edit::Name(_))]),
            "the original field remains usable after recording"
        );
    }
}
