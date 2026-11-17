use super::{Action, frame, label, line};
use crate::{
    config::Config,
    markets::{Point as PricePoint, Quote, State},
};
use cosmic::Element;
use cosmic::iced::{Color, Length, Point, Rectangle, mouse, widget::canvas};
use cosmic::widget::{button, column, row};

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
    let height = size.unwrap_or([360., 420.])[1];
    let mut title = row::with_children(vec![
        label("Markets", 18., p.foreground),
        cosmic::iced::widget::space().width(Length::Fill).into(),
    ]);
    if let Some(action) = action {
        title = title.push(button::text("Edit").on_press(action(Action::MarketsOpen)));
    }
    let mut content = column::with_children(vec![title.into()]).spacing(8);
    if state.quotes.is_empty() {
        let message = state
            .problem
            .as_ref()
            .map(|p| p.text())
            .unwrap_or(if state.loading {
                "Fetching your watchlist…"
            } else {
                "Connect Twelve Data to see your stocks."
            });
        content = content.push(label(message, 13., p.foreground));
        if let Some(action) = action {
            content =
                content.push(button::text("Set up Markets").on_press(action(Action::MarketsOpen)));
        }
    }
    for symbol in &preferences.symbols {
        let Some(quote) = state.quotes.iter().find(|q| &q.symbol == symbol) else {
            continue;
        };
        let price = if quote.price < 1. {
            format!("{:.4} {}", quote.price, quote.currency)
        } else {
            format!("{:.2} {}", quote.price, quote.currency)
        };
        let change = quote
            .percent
            .map(|n| format!("{n:+.2}%"))
            .unwrap_or_else(|| "Change unavailable".into());
        let name: Element<'_, M> = if let Some(action) = action {
            button::text(symbol.clone())
                .on_press(action(Action::MarketSelect(symbol.clone())))
                .into()
        } else {
            label(symbol, 15., p.foreground)
        };
        content = content.push(
            row::with_children(vec![
                name,
                cosmic::iced::widget::space().width(Length::Fill).into(),
                label(price, 14., p.foreground),
                label(change, 13., p.foreground),
            ])
            .spacing(8),
        );
    }
    if let Some(quote) = state
        .quotes
        .iter()
        .find(|q| q.symbol == preferences.selected)
    {
        content = content.push(line(&quote.name, 15., p.foreground));
        if state.chart_symbol == quote.symbol && state.chart.len() > 1 {
            content = content.push(label(
                format!("Last {} trading sessions · daily prices", state.chart.len()),
                12.,
                p.foreground,
            ));
            content = content.push(
                canvas::Canvas::new(Chart {
                    points: &state.chart,
                    color: p.accent,
                })
                .width(Length::Fill)
                .height(90.),
            );
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
            content = content.push(label(
                format!("Low {low:.2}  ·  High {high:.2} {}", quote.currency),
                12.,
                p.foreground,
            ));
            let first = state.chart.first().unwrap().date.format("%b %-d, %Y");
            let last = state.chart.last().unwrap().date.format("%b %-d, %Y");
            content = content.push(label(format!("{first} to {last}"), 12., p.foreground));
        } else {
            content = content.push(label(
                if state.loading {
                    "Fetching chart…"
                } else {
                    "Chart unavailable"
                },
                12.,
                p.foreground,
            ));
        }
        let market = match quote.market_open {
            Some(true) => "Market open",
            Some(false) => "Market closed",
            None => "Market status unavailable",
        };
        let at = chrono::DateTime::from_timestamp(quote.timestamp, 0)
            .map(|d| d.format("%b %-d, %H:%M UTC").to_string())
            .unwrap_or_default();
        content = content.push(label(format!("{market} · As of {at}"), 12., p.foreground));
    }
    if !state.quotes.is_empty()
        && let Some(problem) = &state.problem
    {
        content = content.push(label(problem.text(), 12., p.foreground));
    }
    if !state.unavailable.is_empty() {
        content = content.push(label(
            format!("Unavailable: {}", state.unavailable.join(", ")),
            12.,
            p.foreground,
        ));
    }
    content = content.push(label(
        "Twelve Data · prices may be delayed",
        12.,
        p.foreground,
    ));
    frame(
        cosmic::widget::scrollable(content)
            .height(Length::Fill)
            .into(),
        config,
        height,
    )
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
