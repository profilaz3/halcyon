use super::{Action, frame, label, line};
use crate::{
    config::Config,
    markets::{Point as PricePoint, Quote, State, currency_pair},
};
use cosmic::Element;
use cosmic::iced::{Color, Length, Point, Rectangle, mouse, widget::canvas};
use cosmic::widget::{button, column, container, row};

struct Chart<'a> {
    points: &'a [PricePoint],
    color: Color,
    calendar_days: bool,
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
        let points: Vec<Point> = self
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let fraction = if self.calendar_days {
                    let start = self.points[0].date;
                    (p.date - start).num_days() as f32
                        / (self.points.last().unwrap().date - start).num_days().max(1) as f32
                } else {
                    i as f32 / (self.points.len() - 1) as f32
                };
                let x = 3. + fraction * (bounds.width - 6.).max(0.);
                let y = if spread > 0. {
                    (1. - (p.close - low) / spread) as f32
                } else {
                    0.5
                };
                Point::new(x, 5. + y * (bounds.height - 10.).max(0.))
            })
            .collect();
        let last = points[points.len() - 1];
        let trace = |builder: &mut canvas::path::Builder| {
            builder.move_to(points[0]);
            for &point in &points[1..] {
                builder.line_to(point);
            }
        };
        let area = canvas::Path::new(|builder| {
            trace(builder);
            builder.line_to(Point::new(last.x, bounds.height));
            builder.line_to(Point::new(points[0].x, bounds.height));
            builder.close();
        });
        frame.fill(
            &area,
            Color {
                a: 0.09,
                ..self.color
            },
        );
        let path = canvas::Path::new(trace);
        frame.fill(&canvas::Path::circle(last, 2.6), self.color);
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
    let pair = currency_pair(&preferences.selected);
    let compact = (width < 320. || (width < 420. && height < 340.))
        && preferences
            .symbols
            .iter()
            .any(|s| currency_pair(s).is_some());
    let selected = state
        .quotes
        .iter()
        .find(|q| q.symbol == preferences.selected);
    let gap = if detailed { 6 } else { 4 };
    let row_height = if detailed { 26. } else { 22. };
    let mut title = row::with_children(vec![
        line("Markets", 18., p.foreground),
        cosmic::iced::widget::space().width(Length::Fill).into(),
    ])
    .height(26.);
    if compact && preferences.symbols.len() > 1 {
        let position = preferences
            .symbols
            .iter()
            .position(|s| s == &preferences.selected)
            .unwrap_or(0);
        let next = &preferences.symbols[(position + 1) % preferences.symbols.len()];
        title = title.push(
            button::custom(line("Next", 12., p.foreground))
                .class(cosmic::theme::Button::Text)
                .padding([2, 6])
                .on_press_maybe(action.map(|on| on(Action::MarketSelect(next.clone())))),
        );
    }
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
            if pair.is_some() {
                "ECB · daily reference rate".into()
            } else {
                format!("{} · prices may be delayed", preferences.provider.label())
            },
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
            .unavailable
            .get(&preferences.selected)
            .or(state.problem.as_ref())
            .or_else(|| state.unavailable.values().next())
            .map(|p| p.text())
            .unwrap_or(if state.loading {
                "Fetching your watchlist…"
            } else {
                if pair.is_some() {
                    "ECB reference rates will appear here."
                } else if preferences.provider == crate::markets::Provider::Public {
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
    if compact {
        content = content.push(line(&preferences.selected, 22., p.foreground));
        if let Some(quote) = selected {
            let value = if let Some((base, counter)) = pair {
                format!("1 {base} = {} {counter}", rate(quote.price))
            } else {
                format!("{:.2} {}", quote.price, quote.currency)
            };
            content = content.push(line(value, 17., p.foreground));
            if let Some(change) = quote.percent {
                let reference = if pair.is_some() {
                    "previous rate"
                } else {
                    "previous close"
                };
                content = content.push(line(
                    format!("{change:+.2}% since {reference}"),
                    12.,
                    p.accent,
                ));
            }
        }
    } else {
        let mut watchlist = column::with_children(Vec::new()).spacing(2);
        for symbol in &preferences.symbols {
            let quote = state.quotes.iter().find(|q| &q.symbol == symbol);
            let price = quote
                .map(|q| {
                    if currency_pair(symbol).is_some() {
                        format!("{} {}", rate(q.price), q.currency)
                    } else if q.price < 1. {
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
    }
    if let Some(quote) = selected {
        if detailed {
            let summary = if let Some((base, counter)) = pair {
                format!("1 {base} = {} {counter}", rate(quote.price))
            } else {
                quote.name.clone()
            };
            content = content.push(line(summary, 15., p.foreground));
        }
        if state.chart_symbol == quote.symbol && state.chart.len() > 1 {
            let caption = if pair.is_some() {
                "Last 30 days".into()
            } else if detailed {
                format!("Last {} trading sessions · daily prices", state.chart.len())
            } else {
                format!("{} · {} trading sessions", quote.symbol, state.chart.len())
            };
            content = content.push(line(caption, 12., p.foreground));
            content = content.push(
                canvas::Canvas::new(Chart {
                    points: &state.chart,
                    color: p.accent,
                    calendar_days: pair.is_some(),
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
                    if pair.is_some() {
                        format!("Low {} · High {} {}", rate(low), rate(high), quote.currency)
                    } else {
                        format!("Low {low:.2} · High {high:.2} {}", quote.currency)
                    },
                    12.,
                    p.foreground,
                ));
            }
            let date_format = if detailed { "%b %-d, %Y" } else { "%b %-d" };
            let first = state.chart.first().unwrap().date.format(date_format);
            let last = state.chart.last().unwrap().date.format(date_format);
            if !compact {
                content = content.push(line(format!("{first} to {last}"), 11., p.foreground));
            }
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
        content = content
            .push(container(line("Quote unavailable", 12., p.foreground)).height(Length::Fill));
    }
    // A fixed-height status keeps errors and old quote timestamps visible, too.
    let status = if !state.unavailable.is_empty()
        && !state.unavailable.contains_key(&preferences.selected)
    {
        Some("Some quotes unavailable")
    } else if let Some(problem) = state
        .unavailable
        .get(&preferences.selected)
        .or(state.problem.as_ref())
    {
        use crate::markets::Problem;
        Some(match problem {
            Problem::Setup => "Connect API key",
            Problem::Locked => "Keyring locked",
            Problem::Keyring => "Keyring unavailable",
            Problem::Network => "Offline · cached",
            Problem::RateLimit => "Data limit reached",
            Problem::Provider | Problem::InvalidData => "Data unavailable",
        })
    } else if !state.unavailable.is_empty() {
        Some("Some quotes unavailable")
    } else {
        selected
            .and_then(|q| q.market_open)
            .map(|open| if open { "Market open" } else { "Market closed" })
    };
    let at = selected
        .and_then(|q| chrono::DateTime::from_timestamp(q.timestamp, 0))
        .map(|d| {
            if pair.is_some() {
                d.format("Published %b %-d, %Y").to_string()
            } else {
                d.format("%b %-d, %H:%M UTC").to_string()
            }
        });
    let details = at
        .as_deref()
        .into_iter()
        .chain(status)
        .collect::<Vec<_>>()
        .join(" · ");
    if !details.is_empty() {
        content = content.push(line(details, 11., p.foreground));
    }
    frame(content.push(footer()).into(), config, height)
}

fn rate(value: f64) -> String {
    // ECB publishes varying precision (e.g. 0.83305 GBP). Retain six
    // significant digits for cross rates and at least four decimal places.
    let precision = (5. - value.log10().floor()).clamp(4., 10.) as usize;
    let mut text = format!("{value:.precision$}");
    let minimum = text.find('.').unwrap_or(text.len()) + 5;
    while text.len() > minimum && text.ends_with('0') {
        text.pop();
    }
    text
}

pub fn sample() -> State {
    sample_for(&crate::markets::Preferences::default())
}

pub fn sample_for(preferences: &crate::markets::Preferences) -> State {
    use chrono::{Datelike, Weekday};
    let quotes: Vec<_> = preferences
        .symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| {
            let (name, price, percent) = match symbol.as_str() {
                "AAPL" => ("Apple", 190.12, 1.24),
                "MSFT" => ("Microsoft", 410.32, -0.52),
                "NVDA" => ("NVIDIA", 140.64, 2.15),
                "EUR/USD" => ("EUR/USD", 1.0854, 0.24),
                "EUR/GBP" => ("EUR/GBP", 0.8421, -0.12),
                _ if currency_pair(symbol).is_some() => (symbol.as_str(), 1.2542, 0.31),
                _ => (symbol.as_str(), 100. + index as f64 * 15., 0.8),
            };
            Quote {
                symbol: symbol.clone(),
                name: name.into(),
                currency: currency_pair(symbol)
                    .map_or("USD", |(_, counter)| counter)
                    .into(),
                price,
                change: None,
                percent: Some(percent),
                timestamp: 1794859200,
                market_open: None,
            }
        })
        .collect();
    let selected_price = quotes
        .iter()
        .find(|q| q.symbol == preferences.selected)
        .map_or(100., |q| q.price);
    let end = chrono::DateTime::from_timestamp(1794859200, 0)
        .unwrap()
        .date_naive();
    let mut dates: Vec<_> = (0..)
        .map(|days| end - chrono::Duration::days(days))
        .filter(|d| !matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
        .take(30)
        .collect();
    if currency_pair(&preferences.selected).is_some() {
        dates.retain(|date| (end - *date).num_days() < 30);
    }
    dates.reverse();
    let last = dates.len().saturating_sub(1);
    State {
        source: Some(preferences.clone()),
        quotes,
        chart_symbol: preferences.selected.clone(),
        chart: dates
            .into_iter()
            .enumerate()
            .map(|(i, date)| {
                let remaining = (last - i) as f64;
                PricePoint {
                    date,
                    close: selected_price
                        * (1. - remaining * 0.0015 + (remaining * 0.7).sin() * 0.012),
                }
            })
            .collect(),
        ..State::default()
    }
}
