package harness

import (
	"fmt"

	"github.com/chromedp/cdproto/runtime"
	"github.com/chromedp/chromedp"
)

// Element is a handle on a DOM node: the remote object id the browser gave
// it. Once the document it belongs to is gone, every call on it fails; that
// failure means the node is stale.
type Element struct {
	browser *Browser
	id      runtime.RemoteObjectID
	what    string
}

// By says how to find elements: a CSS selector or an XPath expression.
type By struct {
	kind string
	expr string
}

// CSS finds by CSS selector.
func CSS(selector string) By { return By{kind: "css", expr: selector} }

// XPath finds by XPath 1.0 expression; relative to the scope element when
// the search is made from one.
func XPath(expression string) By { return By{kind: "xpath", expr: expression} }

// ID finds by the id attribute.
func ID(id string) By { return CSS("#" + id) }

// matchesJS is the page-side search shared by every query. It runs with
// `this` as the scope (document or element).
const matchesJS = `const matches = (root, kind, expr) => {
	if (kind === 'css') { return [...root.querySelectorAll(expr)]; }
	const snap = document.evaluate(expr, root, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null);
	const found = [];
	for (let i = 0; i < snap.snapshotLength; i++) { found.push(snap.snapshotItem(i)); }
	return found;
};`

// Query finds the first element matching, and fails when there is none.
func (b *Browser) Query(by By) (*Element, error) {
	return queryIn(b, nil, by)
}

// Optional is Query that reports an absent element as nil, not as an error.
func (b *Browser) Optional(by By) (*Element, error) {
	return optionalIn(b, nil, by)
}

// QueryAll finds every element matching.
func (b *Browser) QueryAll(by By) ([]*Element, error) {
	return queryAllIn(b, nil, by)
}

// Count is how many elements match.
func (b *Browser) Count(by By) (int, error) {
	return valueFrom[int](b, nil, "function(kind, expr) { "+matchesJS+" return matches(this, kind, expr).length; }", by.kind, by.expr)
}

// Find finds the first descendant matching.
func (e *Element) Find(by By) (*Element, error) {
	return queryIn(e.browser, e, by)
}

// FindOptional is Find that reports an absent descendant as nil.
func (e *Element) FindOptional(by By) (*Element, error) {
	return optionalIn(e.browser, e, by)
}

// FindAll finds every descendant matching.
func (e *Element) FindAll(by By) ([]*Element, error) {
	return queryAllIn(e.browser, e, by)
}

// String names the element in messages.
func (e *Element) String() string {
	return e.what
}

func queryIn(b *Browser, scope *Element, by By) (*Element, error) {
	found, err := optionalIn(b, scope, by)
	if err != nil {
		return nil, err
	}
	if found == nil {
		return nil, fmt.Errorf("no element matches %s `%s`", by.kind, by.expr)
	}
	return found, nil
}

func optionalIn(b *Browser, scope *Element, by By) (*Element, error) {
	object, err := objectFrom(b, scope, "function(kind, expr) { "+matchesJS+" return matches(this, kind, expr)[0] ?? null; }", by.kind, by.expr)
	if err != nil {
		return nil, err
	}
	return wrap(b, object, by.expr), nil
}

func queryAllIn(b *Browser, scope *Element, by By) ([]*Element, error) {
	count, err := valueFrom[int](b, scope, "function(kind, expr) { "+matchesJS+" return matches(this, kind, expr).length; }", by.kind, by.expr)
	if err != nil {
		return nil, err
	}
	elements := make([]*Element, 0, count)
	for index := range count {
		object, err := objectFrom(b, scope, "function(kind, expr, index) { "+matchesJS+" return matches(this, kind, expr)[index] ?? null; }", by.kind, by.expr, index)
		if err != nil {
			return nil, err
		}
		found := wrap(b, object, fmt.Sprintf("%s[%d]", by.expr, index))
		if found == nil {
			return nil, fmt.Errorf("`%s` matched %d elements, then fewer", by.expr, count)
		}
		elements = append(elements, found)
	}
	return elements, nil
}

// wrap turns a remote object into an Element, nil for JavaScript null.
func wrap(b *Browser, object *runtime.RemoteObject, what string) *Element {
	if object.ObjectID == "" {
		return nil
	}
	return &Element{browser: b, id: object.ObjectID, what: what}
}

// objectFrom calls function with `this` set to the scope element, or to the
// document when scope is nil, and returns the result as a remote object.
func objectFrom(b *Browser, scope *Element, function string, args ...any) (*runtime.RemoteObject, error) {
	if scope != nil {
		return chromedp.Run(b.ctx, chromedp.CallFunctionOn[*runtime.RemoteObject](function, onObject(scope.id), args...))
	}
	return chromedp.Run(b.ctx, chromedp.Evaluate[*runtime.RemoteObject](onDocument(function, args)))
}

// valueFrom is objectFrom for a result that is a plain value.
func valueFrom[T any](b *Browser, scope *Element, function string, args ...any) (T, error) {
	if scope != nil {
		return chromedp.Run(b.ctx, chromedp.CallFunctionOn[T](function, onObject(scope.id), args...))
	}
	return chromedp.Run(b.ctx, chromedp.Evaluate[T](onDocument(function, args)))
}

func onObject(id runtime.RemoteObjectID) chromedp.CallOption {
	return func(params *runtime.CallFunctionOnParams) {
		params.ObjectID = id
	}
}

func onDocument(function string, args []any) string {
	return fmt.Sprintf("(%s).apply(document, %s)", function, jsLiteral(args))
}

// callOn calls a function with `this` set to the element, and decodes the
// result.
func callOn[T any](e *Element, function string, args ...any) (T, error) {
	return valueFrom[T](e.browser, e, function, args...)
}

// Displayed reports whether the browser renders the element. LiftLog hides
// things with display:none or [hidden], so checkVisibility is enough.
func (e *Element) Displayed() (bool, error) {
	return callOn[bool](e, "function() { return this.checkVisibility({ opacityProperty: true, visibilityProperty: true }); }")
}

// Text is the element's rendered text: a display:none span inside it does not
// contribute.
func (e *Element) Text() (string, error) {
	return callOn[string](e, "function() { return this.innerText.trim(); }")
}

// Attr reads an attribute, and reports whether it is present.
func (e *Element) Attr(name string) (string, bool, error) {
	value, err := callOn[*string](e, "function(name) { return this.getAttribute(name); }", name)
	if err != nil || value == nil {
		return "", false, err
	}
	return *value, true, nil
}

// Value is the current value of a form control.
func (e *Element) Value() (string, error) {
	return callOn[string](e, "function() { return this.value; }")
}

// Connected reports whether the element is still in its document.
func (e *Element) Connected() (bool, error) {
	return callOn[bool](e, "function() { return this.isConnected; }")
}

// Focus gives the element the keyboard focus.
func (e *Element) Focus() error {
	_, err := callOn[chromedp.Void](e, "function() { this.focus(); }")
	return err
}

// Type focuses the element and types text into it, key by key.
func (e *Element) Type(text string) error {
	if err := e.Focus(); err != nil {
		return err
	}
	if text == "" {
		return nil
	}
	return chromedp.Do(e.browser.ctx, chromedp.KeyEvent(text))
}

// Clear empties a form control and tells the page, as a user deleting its
// text would.
func (e *Element) Clear() error {
	_, err := callOn[chromedp.Void](e, `function() {
		this.value = '';
		this.dispatchEvent(new Event('input', { bubbles: true }));
	}`)
	return err
}

// clickPoint is where a native click lands.
type clickPoint struct {
	Visible bool    `json:"visible"`
	X       float64 `json:"x"`
	Y       float64 `json:"y"`
}

// Click is a native click: the element is
// scrolled into view, then a real mouse press and release go to the centre of
// its box. Whatever covers that point receives them.
func (e *Element) Click() error {
	if err := e.browser.WaitForLoad(); err != nil {
		return err
	}
	point, err := e.scrollToCentre()
	if err != nil {
		return err
	}
	if !point.Visible {
		return fmt.Errorf("element not interactable: %s is not displayed", e)
	}
	return chromedp.Do(e.browser.ctx, chromedp.MouseClickXY(point.X, point.Y))
}

func (e *Element) scrollToCentre() (clickPoint, error) {
	return callOn[clickPoint](e, `function() {
		if (!this.checkVisibility({ opacityProperty: true, visibilityProperty: true })) {
			return { visible: false, x: 0, y: 0 };
		}
		// Instant: an animated scroll is still moving when the box is measured.
		this.scrollIntoView({ block: 'end', inline: 'nearest', behavior: 'instant' });
		const r = this.getBoundingClientRect();
		return { visible: true, x: r.left + r.width / 2, y: r.top + r.height / 2 };
	}`)
}
