//! Page objects, one module per surface, ported from `tests/e2e/steps/*.js`.
//!
//! The old steps leaned on Playwright's role and label selectors — `getByLabel`,
//! `getByRole('button', { name })`, `.filter({ hasText })`. WebDriver has none
//! of those, so this module rebuilds the handful that were actually used:
//!
//! * `getByLabel('Weight')` becomes [`fill`] on the input's `id`. Every form in
//!   LiftLog wires its `<label for>` to an `id`, so the two address the same
//!   element; the id is simply the one WebDriver can see.
//! * `getByRole('button', { name })` becomes [`click_button`], an XPath on the
//!   button's own text. Exact, not substring — a reworded button should fail
//!   loudly rather than quietly match a different one.
//! * `.filter({ hasText })` becomes an XPath predicate on the row's contents,
//!   which is how the set and user rows are picked out.

pub mod auth;
pub mod dashboard;
pub mod exercises;
pub mod settings;
pub mod stats;
pub mod users;
pub mod workouts;

use anyhow::{Context, Result};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;

use crate::browser::{WAIT_INTERVAL, WAIT_TIMEOUT};
use crate::server::url;

/// Navigates to a path on the server under test.
pub async fn goto(driver: &WebDriver, path: &str) -> Result<()> {
    driver.goto(url(path)).await?;
    Ok(())
}

/// The path the browser is on, which is what the URL assertions compare.
pub async fn path(driver: &WebDriver) -> Result<String> {
    Ok(driver.current_url().await?.path().to_string())
}

/// Waits for the element matching `by` to be displayed.
pub async fn displayed(driver: &WebDriver, by: By) -> Result<WebElement> {
    Ok(driver
        .query(by)
        .wait(WAIT_TIMEOUT, WAIT_INTERVAL)
        .and_displayed()
        .first()
        .await?)
}

/// Finds an element, mapping "not there" onto `None` rather than an error.
///
/// `nowait`, because every caller is asking about a page that has already
/// rendered — waiting the full timeout to confirm an absence is the default
/// poller's behaviour, not ours.
pub async fn optional(driver: &WebDriver, by: By) -> Result<Option<WebElement>> {
    Ok(driver.query(by).nowait().first_opt().await?)
}

/// How many elements match, without waiting for any of them.
pub async fn count(driver: &WebDriver, by: By) -> Result<usize> {
    Ok(driver.query(by).nowait().all_from_selector().await?.len())
}

/// Every element matching, without waiting.
pub async fn all(driver: &WebDriver, by: By) -> Result<Vec<WebElement>> {
    Ok(driver.query(by).nowait().all_from_selector().await?)
}

/// The rendered text of the first match, or `None` when nothing matches.
pub async fn text(driver: &WebDriver, by: By) -> Result<Option<String>> {
    match optional(driver, by).await? {
        Some(element) => Ok(Some(element.text().await?)),
        None => Ok(None),
    }
}

/// Fills a form field addressed by its `id`, replacing whatever it holds.
pub async fn fill(driver: &WebDriver, id: &str, value: &str) -> Result<()> {
    let field = displayed(driver, By::Id(id)).await?;
    field.clear().await?;
    field.send_keys(value).await?;
    Ok(())
}

/// Sets a field's value directly, for the inputs typing cannot reach.
///
/// `<input type="date">` is the reason: Chrome parses keystrokes into it
/// through the browser's own locale, so sending "2024-03-14" produces whatever
/// that locale makes of those digits. Assigning `value` is what Playwright's
/// `fill()` did underneath, and the `input`/`change` events are dispatched
/// after it so any listener sees the same sequence a real edit produces.
pub async fn set_value(driver: &WebDriver, id: &str, value: &str) -> Result<()> {
    driver
        .execute(
            r"
            const el = document.getElementById(arguments[0]);
            if (!el) { throw new Error('no element with id ' + arguments[0]); }
            el.value = arguments[1];
            el.dispatchEvent(new Event('input', { bubbles: true }));
            el.dispatchEvent(new Event('change', { bubbles: true }));
            ",
            vec![serde_json::json!(id), serde_json::json!(value)],
        )
        .await?;
    Ok(())
}

/// The DOM text of the first match, ignoring how CSS renders it.
///
/// WebDriver's "Get Element Text" returns *rendered* text, so an element under
/// `text-transform: uppercase` reports characters the document does not
/// contain. Playwright's accessible-name matching read through that transform;
/// `textContent` is the equivalent here.
pub async fn dom_text(driver: &WebDriver, css: &str) -> Result<Option<String>> {
    let value = driver
        .execute(
            r"
            const el = document.querySelector(arguments[0]);
            return el ? el.textContent.trim() : null;
            ",
            vec![serde_json::json!(css)],
        )
        .await?
        .json()
        .as_str()
        .map(ToString::to_string);
    Ok(value)
}

/// A field's current value, for the pre-fill assertions.
pub async fn value_of(driver: &WebDriver, id: &str) -> Result<String> {
    Ok(displayed(driver, By::Id(id))
        .await?
        .value()
        .await?
        .unwrap_or_default())
}

/// Picks a `<select>` option by its visible text.
///
/// Goes through `SelectElement`, which clicks the option rather than assigning
/// `value` — so the `change` listener that draws the "last weight" hint on the
/// workout page fires exactly as it would for a real pick.
pub async fn select_by_label(driver: &WebDriver, id: &str, label: &str) -> Result<()> {
    let element = displayed(driver, By::Id(id)).await?;
    SelectElement::new(&element)
        .await?
        .select_by_exact_text(label)
        .await?;
    Ok(())
}

/// Picks a `<select>` option by its `value`.
///
/// The exercise categories need this: the option's value is the stored key
/// (`chest`) while its text is the display name (`Chest`), and the feature files
/// name the key.
pub async fn select_by_value(driver: &WebDriver, id: &str, value: &str) -> Result<()> {
    let element = displayed(driver, By::Id(id)).await?;
    SelectElement::new(&element)
        .await?
        .select_by_value(value)
        .await?;
    Ok(())
}

/// Clicks a `<button>` by its exact text.
pub async fn click_button(driver: &WebDriver, label: &str) -> Result<()> {
    clickable(
        driver,
        By::XPath(format!("//button[normalize-space(.)={}]", quote(label))),
    )
    .await?
    .click()
    .await?;
    Ok(())
}

/// Clicks an `<a>` by its exact text.
pub async fn click_link(driver: &WebDriver, label: &str) -> Result<()> {
    clickable(
        driver,
        By::XPath(format!("//a[normalize-space(.)={}]", quote(label))),
    )
    .await?
    .click()
    .await?;
    Ok(())
}

/// Clicks an `<a>` by its exact text, within one element.
pub async fn click_link_in(scope: &WebElement, label: &str) -> Result<()> {
    scope
        .query(By::XPath(format!(
            ".//a[normalize-space(.)={}]",
            quote(label)
        )))
        .wait(WAIT_TIMEOUT, WAIT_INTERVAL)
        .and_clickable()
        .first()
        .await?
        .click()
        .await?;
    Ok(())
}

/// Turns off a form's client-side validation.
///
/// Every password field carries `minlength`/`maxlength`, so a scenario
/// submitting a deliberately-invalid password would be blocked by the browser
/// and never reach the server-side check that is the actual control. The old
/// suite set `noValidate` for the same reason.
pub async fn disable_validation(driver: &WebDriver, form_css: &str) -> Result<()> {
    driver
        .execute(
            r"
            const form = document.querySelector(arguments[0]);
            if (!form) { throw new Error('no form matching ' + arguments[0]); }
            form.noValidate = true;
            ",
            vec![serde_json::json!(form_css)],
        )
        .await?;
    Ok(())
}

/// The element's distance from the top of the document.
///
/// Rebuilds the `boundingBox().y` comparison the dashboard-ordering scenario
/// makes: it asserts that the quick actions render above the summary, which is
/// a fact about layout that no selector can express.
pub async fn top_of(driver: &WebDriver, css: &str) -> Result<f64> {
    let top = driver
        .execute(
            r"
            const el = document.querySelector(arguments[0]);
            if (!el) { return null; }
            return el.getBoundingClientRect().top + window.scrollY;
            ",
            vec![serde_json::json!(css)],
        )
        .await?
        .json()
        .as_f64()
        .with_context(|| format!("nothing on the page matches `{css}`"))?;
    Ok(top)
}

/// Waits for the element matching `by` to become clickable.
async fn clickable(driver: &WebDriver, by: By) -> Result<WebElement> {
    Ok(driver
        .query(by)
        .wait(WAIT_TIMEOUT, WAIT_INTERVAL)
        .and_clickable()
        .first()
        .await?)
}

/// Quotes a string for use inside an XPath expression.
///
/// XPath 1.0 has no escape character, so a value containing both kinds of quote
/// has to be assembled with `concat()`. None of the fixture names do, but they
/// carry a random suffix and the failure mode — a silently malformed expression
/// that matches nothing — is not one worth leaving open.
pub(crate) fn quote(value: &str) -> String {
    if !value.contains('\'') {
        return format!("'{value}'");
    }
    if !value.contains('"') {
        return format!("\"{value}\"");
    }
    let parts: Vec<String> = value.split('\'').map(|part| format!("'{part}'")).collect();
    format!("concat({})", parts.join(", \"'\", "))
}
