package harness

import (
	"fmt"
	"strings"
)

// The page objects address fields by id (every <label for> points at one) and
// buttons and links by exact text, so a reworded control fails loudly instead
// of matching another.

// Displayed waits for the element matching by to be displayed.
func (b *Browser) Displayed(by By) (*Element, error) {
	return displayedIn(b, nil, by)
}

func displayedIn(b *Browser, scope *Element, by By) (*Element, error) {
	var found *Element
	err := Eventually(fmt.Sprintf("`%s` to be displayed", by.expr), func() (bool, error) {
		element, err := optionalIn(b, scope, by)
		if err != nil || element == nil {
			return false, err
		}
		shown, err := element.Displayed()
		if err != nil || !shown {
			return false, err
		}
		found = element
		return true, nil
	})
	return found, err
}

// TextOf is the rendered text of the first match, with false when nothing
// matches. It does not wait: callers ask about an already-rendered page.
func (b *Browser) TextOf(by By) (string, bool, error) {
	element, err := b.Optional(by)
	if err != nil || element == nil {
		return "", false, err
	}
	text, err := element.Text()
	return text, err == nil, err
}

// TextsOf is the rendered text of every match.
func (b *Browser) TextsOf(by By) ([]string, error) {
	elements, err := b.QueryAll(by)
	if err != nil {
		return nil, err
	}
	return textsOf(elements)
}

func textsOf(elements []*Element) ([]string, error) {
	texts := make([]string, 0, len(elements))
	for _, element := range elements {
		text, err := element.Text()
		if err != nil {
			return nil, err
		}
		texts = append(texts, text)
	}
	return texts, nil
}

// DomText is the textContent of the first CSS match, with false when nothing
// matches. Unlike rendered text, it ignores text-transform.
func (b *Browser) DomText(css string) (string, bool, error) {
	text, err := Eval[*string](b, `(() => {
		const el = document.querySelector(`+jsLiteral(css)+`);
		return el ? el.textContent.trim() : null;
	})()`)
	if err != nil || text == nil {
		return "", false, err
	}
	return *text, true, nil
}

// Fill types into the form field with the id, replacing whatever it holds.
func (b *Browser) Fill(id, value string) error {
	field, err := b.Displayed(ID(id))
	if err != nil {
		return err
	}
	if err := field.Clear(); err != nil {
		return err
	}
	return field.Type(value)
}

// SetValue sets a field's value directly and fires input and change. Needed
// for <input type="date">, where Chrome parses keystrokes through the locale.
func (b *Browser) SetValue(id, value string) error {
	found, err := Eval[bool](b, `(() => {
		const el = document.getElementById(`+jsLiteral(id)+`);
		if (!el) { return false; }
		el.value = `+jsLiteral(value)+`;
		el.dispatchEvent(new Event('input', { bubbles: true }));
		el.dispatchEvent(new Event('change', { bubbles: true }));
		return true;
	})()`)
	if err != nil {
		return err
	}
	if !found {
		return fmt.Errorf("no element with id `%s`", id)
	}
	return nil
}

// ValueOf is a field's current value.
func (b *Browser) ValueOf(id string) (string, error) {
	field, err := b.Displayed(ID(id))
	if err != nil {
		return "", err
	}
	return field.Value()
}

// SelectByLabel picks a <select> option by its visible text, firing change so
// the page's listeners run.
func (b *Browser) SelectByLabel(id, label string) error {
	return b.selectOption(id, "option.textContent.trim()", label)
}

// SelectByValue picks a <select> option by its value, such as the category
// key chest.
func (b *Browser) SelectByValue(id, value string) error {
	return b.selectOption(id, "option.value", value)
}

func (b *Browser) selectOption(id, property, wanted string) error {
	if _, err := b.Displayed(ID(id)); err != nil {
		return err
	}
	found, err := Eval[bool](b, `(() => {
		const select = document.getElementById(`+jsLiteral(id)+`);
		const option = [...select.options].find((option) => `+property+` === `+jsLiteral(wanted)+`);
		if (!option) { return false; }
		select.value = option.value;
		select.dispatchEvent(new Event('input', { bubbles: true }));
		select.dispatchEvent(new Event('change', { bubbles: true }));
		return true;
	})()`)
	if err != nil {
		return err
	}
	if !found {
		return fmt.Errorf("`%s` has no option `%s`", id, wanted)
	}
	return nil
}

// DisableValidation turns off a form's client-side validation, so a
// deliberately invalid password reaches the server-side check.
func (b *Browser) DisableValidation(formCSS string) error {
	found, err := Eval[bool](b, `(() => {
		const form = document.querySelector(`+jsLiteral(formCSS)+`);
		if (!form) { return false; }
		form.noValidate = true;
		return true;
	})()`)
	if err != nil {
		return err
	}
	if !found {
		return fmt.Errorf("no form matching `%s`", formCSS)
	}
	return nil
}

// TopOf is the element's distance from the top of the document, for
// layout-order assertions.
func (b *Browser) TopOf(css string) (float64, error) {
	top, err := Eval[*float64](b, `(() => {
		const el = document.querySelector(`+jsLiteral(css)+`);
		if (!el) { return null; }
		return el.getBoundingClientRect().top + window.scrollY;
	})()`)
	if err != nil {
		return 0, err
	}
	if top == nil {
		return 0, fmt.Errorf("nothing on the page matches `%s`", css)
	}
	return *top, nil
}

// ClickButton clicks a <button> by its exact text.
func (b *Browser) ClickButton(label string) error {
	return b.ClickNavigating(XPath("//button[normalize-space(.)=" + Quote(label) + "]"))
}

// ClickLink clicks an <a> by its exact text.
func (b *Browser) ClickLink(label string) error {
	return b.ClickNavigating(XPath("//a[normalize-space(.)=" + Quote(label) + "]"))
}

// ClickNavigating clicks the control and waits for the page to navigate: a
// click may return before the redirect, and the next navigation would cancel
// the in-flight POST.
func (b *Browser) ClickNavigating(by By) error {
	return clickNavigatingIn(b, nil, by)
}

// ClickNavigatingIn is ClickNavigating for a control inside the element.
func (e *Element) ClickNavigatingIn(by By) error {
	return clickNavigatingIn(e.browser, e, by)
}

// ClickLinkIn clicks an <a> by its exact text, within the element.
func (e *Element) ClickLinkIn(label string) error {
	return e.ClickNavigatingIn(XPath(".//a[normalize-space(.)=" + Quote(label) + "]"))
}

func clickNavigatingIn(b *Browser, scope *Element, by By) error {
	before, err := b.DocumentID()
	if err != nil {
		return err
	}
	return clickUntil(b, clickSpec{
		what:   fmt.Sprintf("`%s`", by.expr),
		locate: func() (*Element, error) { return displayedIn(b, scope, by) },
		tookEffect: func() (bool, error) {
			current, err := b.DocumentID()
			return current != before, err
		},
	})
}

// Quote quotes a string for XPath 1.0, which has no escape character: a value
// with both quote kinds is built with concat().
func Quote(value string) string {
	if !strings.Contains(value, "'") {
		return "'" + value + "'"
	}
	if !strings.Contains(value, `"`) {
		return `"` + value + `"`
	}
	parts := strings.Split(value, "'")
	for i, part := range parts {
		parts[i] = "'" + part + "'"
	}
	return "concat(" + strings.Join(parts, `, "'", `) + ")"
}
