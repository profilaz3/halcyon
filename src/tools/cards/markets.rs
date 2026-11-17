use super::{Action, frame, label, line};
use crate::{
    config::Config,
    markets::{Point as PricePoint, Quote, State},
};
use cosmic::Element;
use cosmic::iced::{Color, Length, Point, Rectangle, mouse, widget::canvas};
use cosmic::widget::{button, column, container, row};

struct Chart<'a> {
    points: &'a [PricePoint],
    color: Color,
}
impl<M> canvas::Program<M, cosmic::Theme> for Chart<'_> {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &cosmic::Renderer,
        _: &cosmic::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        if self.points.len() < 2 {
            return vec![frame.into_geometry()];
        }
        let low = self
            .points
            .iter()
            .map(|p| p.close)
            .fold(f64::INFINITY, f64::min);
        let high = self
            .points
            .iter()
            .map(|p| p.close)
            .fold(f64::NEG_INFINITY, f64::max);
        let spread = high - low;
        let path = canvas::Path::new(|path| {
            for (i, p) in self.points.iter().enumerate() {
                let x =
                    3. + i as f32 / (self.points.len() - 1) as f32 * (bounds.width - 6.).max(0.);
                let y = if spread > 0. {
                    (1. - (p.close - low) / spread) as f32
                } else {
                    0.5
                };
                let point = Point::new(x, 5. + y * (bounds.height - 10.).max(0.));
                if i == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
        });
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_width(2.)
                .with_color(self.color),
        );
        vec![frame.into_geometry()]
    }
}

pub fn markets<'a, M: Clone + 'static>(
    state: &'a State,
    config: &'a Config,
    action: Option<fn(Action) -> M>,
    size: Option<[f32; 2]>,
) -> Element<'a, M> {
    let _scope = crate::widget_style::scope(config);
    let p = config.palette();
    let preferences = if action.is_none() {
        state.source.as_ref().unwrap_or(&config.markets)
    } else {
        &config.markets
    };
    let [width, height] = size.unwrap_or([360., 420.]);
    // Reserve room for every stock before adding optional chart details.
    let detailed =
        width >= 320. && height >= 360. + preferences.symbols.len().saturating_sub(3) as f32 * 22.;
    let gap = if detailed { 6 } else { 4 };
    let row_height = if detailed { 26. } else { 22. };
    let mut title = row::with_children(vec![
        line("Markets", 18., p.foreground),
        cosmic::iced::widget::space().width(Length::Fill).into(),
    ])
    .height(26.);
    if let Some(action) = action {
        title = title.push(
            button::custom(line("Edit", 12., p.foreground))
                .class(cosmic::theme::Button::Text)
                .padding([2, 4])
                .on_press(action(Action::MarketsOpen)),
        );
    }
    let footer = || {
        line(
            format!(
                "{} · prices may be delayed",
                config.markets.provider.label()
            ),
            11.,
            p.foreground,
        )
    };
    let mut content = column::with_children(vec![title.into()])
        .spacing(gap)
        .width(Length::Fill)
        .height(Length::Fill);
    if state.quotes.is_empty() {
        let message = state
            .problem
            .as_ref()
            .map(|p| p.text())
            .unwrap_or(if state.loading {
                "Fetching your watchlist…"
            } else {
                if config.markets.provider == crate::markets::Provider::Public {
                    "Public stock data will appear here. No API key needed."
                } else {
                    "Connect Twelve Data to see your stocks."
                }
            });
        content = content.push(label(message, 13., p.foreground));
        if let Some(action) = action {
            content =
                content.push(button::text("Set up Markets").on_press(action(Action::MarketsOpen)));
        }
        content = content.push(cosmic::iced::widget::space().height(Length::Fill));
        return frame(content.push(footer()).into(), config, height);
    }
    let mut watchlist = column::with_children(Vec::new()).spacing(2);
    for symbol in &preferences.symbols {
        let quote = state.quotes.iter().find(|q| &q.symbol == symbol);
        let price = quote
            .map(|q| {
                if q.price < 1. {
                    format!("{:.4} {}", q.price, q.currency)
                } else {
                    format!("{:.2} {}", q.price, q.currency)
                }
            })
            .unwrap_or_else(|| "No quote".into());
        let change = quote
            .and_then(|q| q.percent)
            .map(|n| format!("{n:+.2}%"))
            .unwrap_or_else(|| "n/a".into());
        let ink = if symbol == &preferences.selected {
            p.accent
        } else {
            p.foreground
        };
        let cells = row::with_children(vec![
            container(line(symbol, 13., ink))
                .width(Length::FillPortion(3))
                .into(),
            container(line(price, 12., p.foreground))
                .width(Length::FillPortion(5))
                .align_x(cosmic::iced::Alignment::End)
                .into(),
            container(line(change, 11., p.foreground))
                .width(Length::FillPortion(2))
                .align_x(cosmic::iced::Alignment::End)
                .into(),
        ])
        .spacing(4)
        .align_y(cosmic::iced::Alignment::Center);
        // The whole bounded row selects the stock, including at compact sizes.
        watchlist = watchlist.push(
            button::custom(cells)
                .class(cosmic::theme::Button::Text)
                .padding([2, 0])
                .width(Length::Fill)
                .height(row_height)
                .on_press_maybe(action.map(|on| on(Action::MarketSelect(symbol.clone())))),
        );
    }
    content = content.push(watchlist);
    let selected = state
        .quotes
        .iter()
        .find(|q| q.symbol == preferences.selected);
    if let Some(quote) = selected {
        if detailed {
            content = content.push(line(&quote.name, 15., p.foreground));
        }
        if state.chart_symbol == quote.symbol && state.chart.len() > 1 {
            let caption = if detailed {
                format!("Last {} trading sessions · daily prices", state.chart.len())
            } else {
                format!("{} · {} trading sessions", quote.symbol, state.chart.len())
            };
            content = content.push(line(caption, 12., p.foreground));
            content = content.push(
                canvas::Canvas::new(Chart {
                    points: &state.chart,
                    color: p.accent,
                })
                .width(Length::Fill)
                .height(Length::Fill),
            );
            if detailed {
                let low = state
                    .chart
                    .iter()
                    .map(|p| p.close)
                    .fold(f64::INFINITY, f64::min);
                let high = state
                    .chart
                    .iter()
                    .map(|p| p.close)
                    .fold(f64::NEG_INFINITY, f64::max);
                content = content.push(line(
                    format!("Low {low:.2} · High {high:.2} {}", quote.currency),
                    12.,
                    p.foreground,
                ));
            }
            let date_format = if detailed { "%b %-d, %Y" } else { "%b %-d" };
            let first = state.chart.first().unwrap().date.format(date_format);
            let last = state.chart.last().unwrap().date.format(date_format);
            content = content.push(line(format!("{first} to {last}"), 11., p.foreground));
        } else {
            content = content.push(
                container(line(
                    if state.loading {
                        "Fetching chart…"
                    } else {
                        "Chart unavailable"
                    },
                    12.,
                    p.foreground,
                ))
                .height(Length::Fill),
            );
        }
    } else {
        content = content.push(
            container(line("Select an available stock", 12., p.foreground)).height(Length::Fill),
        );
    }
    // A fixed-height status keeps errors and old quote timestamps visible, too.
    let status = if let Some(problem) = &state.problem {
        use crate::markets::Problem;
        match problem {
            Problem::Setup => "Connect API key",
            Problem::Locked => "Keyring locked",
            Problem::Keyring => "Keyring unavailable",
            Problem::Network => "Offline · cached",
            Problem::RateLimit => "Data limit reached",
            Problem::Provider | Problem::InvalidData => "Data unavailable",
        }
    } else if !state.unavailable.is_empty() {
        "Some quotes unavailable"
    } else {
        match selected.and_then(|q| q.market_open) {
            Some(true) => "Market open",
            Some(false) => "Market closed",
            None => "Status unavailable",
        }
    };
    let at = selected
        .and_then(|q| chrono::DateTime::from_timestamp(q.timestamp, 0))
        .map(|d| d.format("%b %-d, %H:%M UTC").to_string());
    content = content.push(line(
        at.map(|at| format!("{at} · {status}"))
            .unwrap_or_else(|| status.into()),
        11.,
        p.foreground,
    ));
    frame(content.push(footer()).into(), config, height)
}

pub fn sample() -> State {
    let quotes = [
        ("AAPL", "Apple", 190.12, 1.24),
        ("MSFT", "Microsoft", 410.32, -0.52),
        ("NVDA", "NVIDIA", 140.64, 2.15),
    ]
    .into_iter()
    .map(|(symbol, name, price, percent)| Quote {
        symbol: symbol.into(),
        name: name.into(),
        currency: "USD".into(),
        price,
        change: None,
        percent: Some(percent),
        timestamp: 1794859200,
        market_open: Some(false),
    })
    .collect();
    State {
        source: Some(crate::markets::Preferences::default()),
        quotes,
        chart_symbol: "AAPL".into(),
        chart: (0..30)
            .map(|i| PricePoint {
                date: chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
                    + chrono::Duration::days(i),
                close: 180. + i as f64 * 0.3 + (i as f64 * 0.7).sin() * 2.,
            })
            .collect(),
        ..State::default()
    }
}
