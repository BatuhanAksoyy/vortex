//! How heavy the page is, as its renderer counts it.

use serde_json::{Value, json};

use crate::Page;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageMetrics {
    /// Bytes the page's scripts use, of those set aside for them.
    pub heap_used: u64,
    pub heap_total: u64,
    pub nodes: u64,
    pub listeners: u64,
    pub documents: u64,
}

impl Page {
    /// # Errors
    /// When the browser does not answer.
    pub async fn metrics(&self) -> anyhow::Result<PageMetrics> {
        let reply = self.call("Performance.getMetrics", json!({})).await?;
        Ok(read(&reply["metrics"]))
    }
}

fn read(metrics: &Value) -> PageMetrics {
    let mut read = PageMetrics::default();
    for metric in metrics.as_array().map_or_default(Vec::as_slice) {
        let value = metric["value"].as_f64().unwrap_or_default();
        // Counts and byte sizes, never negative or fractional.
        let value = format!("{:.0}", value.max(0.)).parse().unwrap_or(0);
        let field = match metric["name"].as_str() {
            Some("JSHeapUsedSize") => &mut read.heap_used,
            Some("JSHeapTotalSize") => &mut read.heap_total,
            Some("Nodes") => &mut read.nodes,
            Some("JSEventListeners") => &mut read.listeners,
            Some("Documents") => &mut read.documents,
            _ => continue,
        };
        *field = value;
    }
    read
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_metrics_shown_are_picked_out() {
        let metrics = json!([
            {"name": "Nodes", "value": 120.0},
            {"name": "JSHeapUsedSize", "value": 2048.4},
            {"name": "LayoutCount", "value": 9.0},
        ]);
        let read = read(&metrics);
        assert_eq!((read.nodes, read.heap_used, read.listeners), (120, 2048, 0));
    }
}
