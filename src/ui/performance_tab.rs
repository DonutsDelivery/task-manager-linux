use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;

use crate::model::SystemSnapshot;
use crate::ui::graph_widget::{GraphColor, GraphWidget};
use crate::util;

pub struct PerformanceTab {
    pub widget: gtk::Box,
    stack: gtk::Stack,
    cpu_panel: CpuPanel,
    memory_panel: MemoryPanel,
    gpu_panel: GpuPanel,
    disk_panel: DiskPanel,
    network_panel: NetworkPanel,
}

impl PerformanceTab {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        // Sub-navigation sidebar
        let nav_list = gtk::ListBox::new();
        nav_list.set_selection_mode(gtk::SelectionMode::Single);
        nav_list.add_css_class("perf-sidebar");

        let items = ["CPU", "Memory", "GPU", "Disk", "Network"];
        for name in &items {
            let row = gtk::Label::new(Some(name));
            row.set_halign(gtk::Align::Start);
            row.set_margin_top(6);
            row.set_margin_bottom(6);
            row.set_margin_start(12);
            row.set_margin_end(12);
            nav_list.append(&row);
        }

        let nav_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(140)
            .child(&nav_list)
            .build();

        // Content stack
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_vexpand(true);
        stack.set_hexpand(true);

        let cpu_panel = CpuPanel::new();
        stack.add_named(&cpu_panel.widget, Some("cpu"));

        let memory_panel = MemoryPanel::new();
        stack.add_named(&memory_panel.widget, Some("memory"));

        let gpu_panel = GpuPanel::new();
        stack.add_named(&gpu_panel.widget, Some("gpu"));

        let disk_panel = DiskPanel::new();
        stack.add_named(&disk_panel.widget, Some("disk"));

        let network_panel = NetworkPanel::new();
        stack.add_named(&network_panel.widget, Some("network"));

        let stack_ref = stack.clone();
        let names = ["cpu", "memory", "gpu", "disk", "network"];
        nav_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let idx = row.index() as usize;
                if let Some(name) = names.get(idx) {
                    stack_ref.set_visible_child_name(name);
                }
            }
        });

        // Select first row
        if let Some(first) = nav_list.row_at_index(0) {
            nav_list.select_row(Some(&first));
        }

        widget.append(&nav_scroll);
        widget.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        widget.append(&stack);

        Self {
            widget,
            stack,
            cpu_panel,
            memory_panel,
            gpu_panel,
            disk_panel,
            network_panel,
        }
    }

    pub fn update(&mut self, snapshot: &SystemSnapshot) {
        self.cpu_panel.update(&snapshot.cpu);
        self.memory_panel.update(&snapshot.memory);
        self.gpu_panel.update(&snapshot.gpu);
        self.disk_panel.update(&snapshot.disk);
        self.network_panel.update(&snapshot.network);
    }
}

// ── CPU Panel ─────────────────────────────────────────────

struct CpuPanel {
    widget: gtk::Box,
    graph: GraphWidget,
    title_label: gtk::Label,
    utilization_label: gtk::Label,
    speed_label: gtk::Label,
    cores_label: gtk::Label,
    uptime_label: gtk::Label,
    initialized: bool,
}

impl CpuPanel {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_margin_top(16);
        widget.set_margin_start(16);
        widget.set_margin_end(16);
        widget.set_margin_bottom(16);

        let title_label = gtk::Label::new(Some("CPU"));
        title_label.add_css_class("perf-label-title");
        title_label.set_halign(gtk::Align::Start);

        let graph = GraphWidget::new(600, 200);
        graph.set_series_count(1, vec![GraphColor::new(0.2, 0.6, 1.0)]);
        graph.set_max_value(100.0);

        let info_grid = gtk::Grid::new();
        info_grid.set_row_spacing(6);
        info_grid.set_column_spacing(24);

        let utilization_label = gtk::Label::new(Some("0%"));
        let speed_label = gtk::Label::new(Some("0 GHz"));
        let cores_label = gtk::Label::new(Some("0"));
        let uptime_label = gtk::Label::new(Some("0m"));

        add_info_row(&info_grid, 0, "Utilization", &utilization_label);
        add_info_row(&info_grid, 1, "Speed", &speed_label);
        add_info_row(&info_grid, 2, "Cores", &cores_label);
        add_info_row(&info_grid, 3, "Uptime", &uptime_label);

        widget.append(&title_label);
        widget.append(&graph.widget);
        widget.append(&info_grid);

        Self {
            widget,
            graph,
            title_label,
            utilization_label,
            speed_label,
            cores_label,
            uptime_label,
            initialized: false,
        }
    }

    fn update(&mut self, cpu: &crate::model::CpuInfo) {
        if !self.initialized && !cpu.model_name.is_empty() {
            self.title_label.set_text(&format!("CPU — {}", cpu.model_name));
            self.cores_label.set_text(&format!("{} cores", cpu.core_count));
            self.initialized = true;
        }

        self.graph.push_single(cpu.total_percent);
        self.utilization_label.set_text(&util::format_percent(cpu.total_percent));
        self.speed_label.set_text(&util::format_frequency(cpu.frequency_mhz));
        self.uptime_label.set_text(&util::format_duration(cpu.uptime_secs));
    }
}

// ── Memory Panel ──────────────────────────────────────────

struct MemoryPanel {
    widget: gtk::Box,
    graph: GraphWidget,
    used_label: gtk::Label,
    available_label: gtk::Label,
    cached_label: gtk::Label,
    swap_label: gtk::Label,
    total_label: gtk::Label,
    initialized: bool,
}

impl MemoryPanel {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_margin_top(16);
        widget.set_margin_start(16);
        widget.set_margin_end(16);
        widget.set_margin_bottom(16);

        let title = gtk::Label::new(Some("Memory"));
        title.add_css_class("perf-label-title");
        title.set_halign(gtk::Align::Start);

        let graph = GraphWidget::new(600, 200);
        graph.set_series_count(1, vec![GraphColor::new(0.6, 0.2, 0.8)]);

        let info_grid = gtk::Grid::new();
        info_grid.set_row_spacing(6);
        info_grid.set_column_spacing(24);

        let used_label = gtk::Label::new(Some("0 B"));
        let available_label = gtk::Label::new(Some("0 B"));
        let cached_label = gtk::Label::new(Some("0 B"));
        let swap_label = gtk::Label::new(Some("0 B"));
        let total_label = gtk::Label::new(Some("0 B"));

        add_info_row(&info_grid, 0, "Used", &used_label);
        add_info_row(&info_grid, 1, "Available", &available_label);
        add_info_row(&info_grid, 2, "Cached", &cached_label);
        add_info_row(&info_grid, 3, "Swap", &swap_label);
        add_info_row(&info_grid, 4, "Total", &total_label);

        widget.append(&title);
        widget.append(&graph.widget);
        widget.append(&info_grid);

        Self {
            widget,
            graph,
            used_label,
            available_label,
            cached_label,
            swap_label,
            total_label,
            initialized: false,
        }
    }

    fn update(&mut self, mem: &crate::model::MemoryInfo) {
        if !self.initialized && mem.total > 0 {
            self.graph.set_max_value(mem.total as f64);
            self.total_label.set_text(&util::format_bytes(mem.total));
            self.initialized = true;
        }

        self.graph.push_single(mem.used as f64);
        self.used_label.set_text(&util::format_bytes(mem.used));
        self.available_label.set_text(&util::format_bytes(mem.available));
        self.cached_label.set_text(&util::format_bytes(mem.cached));
        self.swap_label.set_text(&format!(
            "{} / {}",
            util::format_bytes(mem.swap_used),
            util::format_bytes(mem.swap_total)
        ));
    }
}

// ── GPU Panel ─────────────────────────────────────────────

struct GpuPanel {
    widget: gtk::Box,
    graph: GraphWidget,
    title_label: gtk::Label,
    utilization_label: gtk::Label,
    vram_label: gtk::Label,
    temp_label: gtk::Label,
    power_label: gtk::Label,
    fan_label: gtk::Label,
    no_gpu_label: gtk::Label,
    initialized: bool,
}

impl GpuPanel {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_margin_top(16);
        widget.set_margin_start(16);
        widget.set_margin_end(16);
        widget.set_margin_bottom(16);

        let title_label = gtk::Label::new(Some("GPU"));
        title_label.add_css_class("perf-label-title");
        title_label.set_halign(gtk::Align::Start);

        let no_gpu_label = gtk::Label::new(Some("No NVIDIA GPU detected"));
        no_gpu_label.set_halign(gtk::Align::Start);

        let graph = GraphWidget::new(600, 200);
        graph.set_series_count(2, vec![
            GraphColor::new(0.2, 0.8, 0.4), // Utilization
            GraphColor::new(0.8, 0.4, 0.2), // VRAM
        ]);
        graph.set_max_value(100.0);

        let info_grid = gtk::Grid::new();
        info_grid.set_row_spacing(6);
        info_grid.set_column_spacing(24);

        let utilization_label = gtk::Label::new(Some("0%"));
        let vram_label = gtk::Label::new(Some("0 B"));
        let temp_label = gtk::Label::new(Some("0 C"));
        let power_label = gtk::Label::new(Some("0 W"));
        let fan_label = gtk::Label::new(Some("0%"));

        add_info_row(&info_grid, 0, "Utilization", &utilization_label);
        add_info_row(&info_grid, 1, "VRAM", &vram_label);
        add_info_row(&info_grid, 2, "Temperature", &temp_label);
        add_info_row(&info_grid, 3, "Power", &power_label);
        add_info_row(&info_grid, 4, "Fan Speed", &fan_label);

        widget.append(&title_label);
        widget.append(&no_gpu_label);
        widget.append(&graph.widget);
        widget.append(&info_grid);

        Self {
            widget,
            graph,
            title_label,
            utilization_label,
            vram_label,
            temp_label,
            power_label,
            fan_label,
            no_gpu_label,
            initialized: false,
        }
    }

    fn update(&mut self, gpu: &crate::model::GpuInfo) {
        if gpu.available {
            self.no_gpu_label.set_visible(false);
            self.graph.widget.set_visible(true);

            if !self.initialized {
                self.title_label.set_text(&format!("GPU — {}", gpu.name));
                self.initialized = true;
            }

            let vram_pct = if gpu.vram_total > 0 {
                (gpu.vram_used as f64 / gpu.vram_total as f64) * 100.0
            } else {
                0.0
            };

            self.graph.push_values(&[gpu.utilization_percent, vram_pct]);
            self.utilization_label.set_text(&util::format_percent(gpu.utilization_percent));
            self.vram_label.set_text(&format!(
                "{} / {}",
                util::format_bytes(gpu.vram_used),
                util::format_bytes(gpu.vram_total)
            ));
            self.temp_label.set_text(&format!("{} C", gpu.temperature));
            self.power_label.set_text(&format!(
                "{:.0} W / {:.0} W",
                gpu.power_watts, gpu.power_limit_watts
            ));
            self.fan_label.set_text(&format!("{}%", gpu.fan_speed_percent));
        } else {
            self.no_gpu_label.set_visible(true);
            self.graph.widget.set_visible(false);
        }
    }
}

// ── Disk Panel ────────────────────────────────────────────

struct DiskPanel {
    widget: gtk::Box,
    graph: GraphWidget,
    info_label: gtk::Label,
}

impl DiskPanel {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_margin_top(16);
        widget.set_margin_start(16);
        widget.set_margin_end(16);
        widget.set_margin_bottom(16);

        let title = gtk::Label::new(Some("Disk"));
        title.add_css_class("perf-label-title");
        title.set_halign(gtk::Align::Start);

        let graph = GraphWidget::new(600, 200);
        graph.set_series_count(2, vec![
            GraphColor::new(0.2, 0.7, 0.9), // Read
            GraphColor::new(0.9, 0.5, 0.2), // Write
        ]);
        graph.set_max_value(100_000_000.0); // 100 MB/s default scale

        let info_label = gtk::Label::new(Some(""));
        info_label.set_halign(gtk::Align::Start);
        info_label.set_wrap(true);

        widget.append(&title);
        widget.append(&graph.widget);
        widget.append(&info_label);

        Self {
            widget,
            graph,
            info_label,
        }
    }

    fn update(&mut self, disk: &crate::model::DiskInfo) {
        let mut total_read = 0.0f64;
        let mut total_write = 0.0f64;
        let mut info_parts = Vec::new();

        for dev in &disk.devices {
            total_read += dev.read_bytes_sec;
            total_write += dev.write_bytes_sec;
            info_parts.push(format!(
                "{}:  R: {}  W: {}",
                dev.name,
                util::format_bytes_rate(dev.read_bytes_sec),
                util::format_bytes_rate(dev.write_bytes_sec)
            ));
        }

        // Auto-scale: max of current values * 1.5, minimum 1 MB/s
        let max = (total_read.max(total_write) * 1.5).max(1_000_000.0);
        self.graph.set_max_value(max);

        self.graph.push_values(&[total_read, total_write]);
        self.info_label.set_text(&info_parts.join("\n"));
    }
}

// ── Network Panel ─────────────────────────────────────────

struct NetworkPanel {
    widget: gtk::Box,
    graph: GraphWidget,
    info_label: gtk::Label,
}

impl NetworkPanel {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_margin_top(16);
        widget.set_margin_start(16);
        widget.set_margin_end(16);
        widget.set_margin_bottom(16);

        let title = gtk::Label::new(Some("Network"));
        title.add_css_class("perf-label-title");
        title.set_halign(gtk::Align::Start);

        let graph = GraphWidget::new(600, 200);
        graph.set_series_count(2, vec![
            GraphColor::new(0.2, 0.8, 0.5), // Download
            GraphColor::new(0.8, 0.3, 0.3), // Upload
        ]);
        graph.set_max_value(10_000_000.0); // 10 MB/s default

        let info_label = gtk::Label::new(Some(""));
        info_label.set_halign(gtk::Align::Start);
        info_label.set_wrap(true);

        widget.append(&title);
        widget.append(&graph.widget);
        widget.append(&info_label);

        Self {
            widget,
            graph,
            info_label,
        }
    }

    fn update(&mut self, net: &crate::model::NetworkInfo) {
        let mut total_rx = 0.0f64;
        let mut total_tx = 0.0f64;
        let mut info_parts = Vec::new();

        for iface in &net.interfaces {
            total_rx += iface.rx_bytes_sec;
            total_tx += iface.tx_bytes_sec;
            info_parts.push(format!(
                "{}:  DL: {}  UL: {}",
                iface.name,
                util::format_bytes_rate(iface.rx_bytes_sec),
                util::format_bytes_rate(iface.tx_bytes_sec)
            ));
        }

        let max = (total_rx.max(total_tx) * 1.5).max(100_000.0);
        self.graph.set_max_value(max);

        self.graph.push_values(&[total_rx, total_tx]);
        self.info_label.set_text(&info_parts.join("\n"));
    }
}

// ── Helpers ───────────────────────────────────────────────

fn add_info_row(grid: &gtk::Grid, row: i32, label_text: &str, value_label: &gtk::Label) {
    let label = gtk::Label::new(Some(label_text));
    label.set_halign(gtk::Align::Start);
    label.add_css_class("dim-label");
    value_label.set_halign(gtk::Align::Start);
    value_label.add_css_class("perf-label-value");
    grid.attach(&label, 0, row, 1, 1);
    grid.attach(value_label, 1, row, 1, 1);
}
