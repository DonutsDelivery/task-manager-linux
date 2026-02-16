use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::rc::Rc;

const POINTS_1MIN: usize = 60;    // 1 sample/sec for 1 minute
const POINTS_5MIN: usize = 300;   // 1 sample/sec for 5 minutes
const POINTS_30MIN: usize = 1800; // 1 sample/sec for 30 minutes

#[derive(Clone)]
pub struct GraphColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl GraphColor {
    pub fn new(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }
}

pub struct GraphWidget {
    pub widget: gtk::Overlay,
    drawing_area: gtk::DrawingArea,
    data: Rc<RefCell<Vec<VecDeque<f64>>>>,
    colors: Rc<RefCell<Vec<GraphColor>>>,
    labels: Rc<RefCell<Vec<String>>>,
    max_value: Rc<RefCell<f64>>,
    title: Rc<RefCell<String>>,
    window_size: Rc<RefCell<usize>>,
}

impl GraphWidget {
    pub fn new(width: i32, height: i32) -> Self {
        let data: Rc<RefCell<Vec<VecDeque<f64>>>> = Rc::new(RefCell::new(Vec::new()));
        let colors: Rc<RefCell<Vec<GraphColor>>> = Rc::new(RefCell::new(Vec::new()));
        let labels: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let max_value: Rc<RefCell<f64>> = Rc::new(RefCell::new(100.0));
        let title: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let window_size: Rc<RefCell<usize>> = Rc::new(RefCell::new(POINTS_1MIN));

        let area = gtk::DrawingArea::new();
        area.set_content_width(width);
        area.set_content_height(height);
        area.add_css_class("graph-area");

        let data_c = data.clone();
        let colors_c = colors.clone();
        let max_c = max_value.clone();
        let window_c = window_size.clone();

        area.set_draw_func(move |_area, cr, w, h| {
            let w = w as f64;
            let h = h as f64;
            let margin_left = 0.0;
            let margin_right = 4.0;
            let margin_top = 4.0;
            let margin_bottom = 4.0;
            let gw = w - margin_left - margin_right;
            let gh = h - margin_top - margin_bottom;

            // Background
            cr.set_source_rgba(0.1, 0.1, 0.12, 1.0);
            rounded_rect(cr, 0.0, 0.0, w, h, 6.0);
            let _ = cr.fill();

            // Grid lines
            cr.set_source_rgba(0.25, 0.25, 0.28, 1.0);
            cr.set_line_width(0.5);
            for i in 1..4 {
                let y = margin_top + gh * (i as f64 / 4.0);
                cr.move_to(margin_left, y);
                cr.line_to(w - margin_right, y);
                let _ = cr.stroke();
            }
            for i in 1..6 {
                let x = margin_left + gw * (i as f64 / 6.0);
                cr.move_to(x, margin_top);
                cr.line_to(x, h - margin_bottom);
                let _ = cr.stroke();
            }

            // Draw data lines
            let data = data_c.borrow();
            let colors = colors_c.borrow();
            let max = *max_c.borrow();
            let max_points = *window_c.borrow();

            for (series_idx, series) in data.iter().enumerate() {
                if series.is_empty() {
                    continue;
                }
                let color = colors.get(series_idx).cloned().unwrap_or(GraphColor::new(0.3, 0.6, 1.0));

                let n = series.len();
                let step = gw / (max_points as f64 - 1.0);

                // Fill area under curve
                cr.set_source_rgba(color.r, color.g, color.b, 0.15);
                cr.move_to(margin_left + (max_points - n) as f64 * step, margin_top + gh);
                for (i, &val) in series.iter().enumerate() {
                    let x = margin_left + (max_points - n + i) as f64 * step;
                    let y = margin_top + gh - (val / max) * gh;
                    cr.line_to(x, y);
                }
                cr.line_to(margin_left + (max_points - 1) as f64 * step, margin_top + gh);
                cr.close_path();
                let _ = cr.fill();

                // Line
                cr.set_source_rgba(color.r, color.g, color.b, 0.9);
                cr.set_line_width(1.5);
                for (i, &val) in series.iter().enumerate() {
                    let x = margin_left + (max_points - n + i) as f64 * step;
                    let y = margin_top + gh - (val / max) * gh;
                    if i == 0 {
                        cr.move_to(x, y);
                    } else {
                        cr.line_to(x, y);
                    }
                }
                let _ = cr.stroke();
            }
        });

        // Create overlay to hold drawing area and dropdown
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&area));

        // Create dropdown for time window selection
        let dropdown_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dropdown_box.set_halign(gtk::Align::End);
        dropdown_box.set_valign(gtk::Align::Start);
        dropdown_box.set_margin_top(8);
        dropdown_box.set_margin_end(8);

        let time_options = gtk::StringList::new(&["1 min", "5 min", "30 min"]);
        let dropdown = gtk::DropDown::new(Some(time_options), None::<gtk::Expression>);
        dropdown.set_selected(0); // Default to 1 min
        dropdown.add_css_class("graph-time-selector");

        let data_clone = data.clone();
        let window_clone = window_size.clone();
        let area_clone = area.clone();

        dropdown.connect_selected_notify(move |dropdown| {
            let selected = dropdown.selected();
            let new_size = match selected {
                0 => POINTS_1MIN,
                1 => POINTS_5MIN,
                2 => POINTS_30MIN,
                _ => POINTS_1MIN,
            };

            *window_clone.borrow_mut() = new_size;

            // Truncate data if necessary
            let mut data = data_clone.borrow_mut();
            for series in data.iter_mut() {
                while series.len() > new_size {
                    series.pop_front();
                }
            }

            area_clone.queue_draw();
        });

        dropdown_box.append(&dropdown);
        overlay.add_overlay(&dropdown_box);

        Self {
            widget: overlay,
            drawing_area: area,
            data,
            colors,
            labels,
            max_value,
            title,
            window_size,
        }
    }

    pub fn set_series_count(&self, count: usize, colors: Vec<GraphColor>) {
        let mut data = self.data.borrow_mut();
        let window_size = *self.window_size.borrow();
        data.resize_with(count, || VecDeque::with_capacity(window_size));
        *self.colors.borrow_mut() = colors;
    }

    pub fn set_max_value(&self, max: f64) {
        *self.max_value.borrow_mut() = max;
    }

    pub fn push_values(&self, values: &[f64]) {
        let mut data = self.data.borrow_mut();
        let window_size = *self.window_size.borrow();
        for (i, &val) in values.iter().enumerate() {
            if i >= data.len() {
                data.push(VecDeque::with_capacity(window_size));
            }
            let series = &mut data[i];
            series.push_back(val);
            if series.len() > window_size {
                series.pop_front();
            }
        }
        self.drawing_area.queue_draw();
    }

    pub fn push_single(&self, value: f64) {
        self.push_values(&[value]);
    }

    pub fn set_time_window(&self, points: usize) {
        *self.window_size.borrow_mut() = points;

        // Truncate data if reducing window size
        let mut data = self.data.borrow_mut();
        for series in data.iter_mut() {
            while series.len() > points {
                series.pop_front();
            }
        }

        self.drawing_area.queue_draw();
    }
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}
