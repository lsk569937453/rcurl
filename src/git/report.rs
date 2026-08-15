use crate::git::model::GitStatistics;

const TEMPLATE: &str = include_str!("template.html");

/// Render the single-page HTML report. ECharts loads from a CDN, so the file
/// itself stays tiny; the statistics are injected as inline JSON.
pub fn render(stats: &GitStatistics) -> Result<String, anyhow::Error> {
    let data = serde_json::to_string(stats)?;
    // `\/` is a valid JSON escape for `/`, so this keeps the JSON parseable
    // while preventing an author/tag name from closing the script block.
    let data = data.replace("</script", "<\\/script");
    Ok(TEMPLATE
        .replace("__RCURL_TITLE__", &escape_html(&stats.git_base_info.project_name))
        .replace("__RCURL_DATA_JSON__", &data))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}
