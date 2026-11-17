/* @ds-bundle: {"format":4,"namespace":"Flint","components":[{"name":"Button"},{"name":"Input"},{"name":"Tag"},{"name":"Card"},{"name":"Tabs"},{"name":"Menu"},{"name":"BarChart"}]} */
(function () {
  var React = window.React;
  var h = React.createElement;

  function cx() {
    var out = [];
    for (var i = 0; i < arguments.length; i++) if (arguments[i]) out.push(arguments[i]);
    return out.join(' ');
  }
  function omit(obj, keys) {
    var out = {};
    for (var k in obj) if (Object.prototype.hasOwnProperty.call(obj, k) && keys.indexOf(k) < 0) out[k] = obj[k];
    return out;
  }

  /* Button — primary | secondary | ghost | danger; md (32px) | sm (28px). */
  function Button(props) {
    var variant = props.variant || 'secondary';
    var size = props.size || 'md';
    var rest = omit(props, ['variant', 'size', 'className', 'children', 'type']);
    rest.type = props.type || 'button';
    rest.className = cx('fl-btn', 'fl-btn-' + variant, size === 'sm' && 'fl-btn-sm', props.className);
    return h('button', rest, props.children);
  }

  /* Input — a labelled text field with optional hint or error. */
  function Input(props) {
    var autoId = React.useId ? React.useId() : 'fl-input';
    var id = props.id || autoId;
    var hintId = id + '-hint';
    var message = props.error || props.hint;
    var rest = omit(props, ['label', 'hint', 'error', 'id', 'className', 'style']);
    rest.id = id;
    rest.className = 'fl-input';
    if (props.error) rest['aria-invalid'] = 'true';
    if (message) rest['aria-describedby'] = hintId;
    return h('div', { className: cx('fl-field', props.className), style: props.style },
      props.label ? h('label', { className: 'fl-field-label', htmlFor: id }, props.label) : null,
      h('input', rest),
      message ? h('span', { id: hintId, className: cx('fl-field-hint', props.error && 'is-error') }, message) : null
    );
  }

  /* Tag — one to three words of status in mono caps; status tones carry a square marker. */
  function Tag(props) {
    var tone = props.tone || 'neutral';
    var marker = props.marker === undefined ? tone !== 'neutral' : props.marker;
    return h('span', { className: cx('fl-tag', 'fl-tag-' + tone, props.className) },
      marker ? h('span', { className: 'fl-tag-mark', 'aria-hidden': 'true' }) : null,
      props.children
    );
  }

  /* Card — a raised surface: optional eyebrow, title and meta in a header, body, footer actions. */
  function Card(props) {
    var hasHead = props.title || props.eyebrow || props.meta;
    var rest = omit(props, ['eyebrow', 'title', 'meta', 'footer', 'children', 'interactive', 'selected', 'className']);
    rest.className = cx('fl-card', props.interactive && 'fl-card-interactive', props.selected && 'fl-card-selected', props.className);
    if (props.interactive && rest.tabIndex === undefined) rest.tabIndex = 0;
    return h('section', rest,
      hasHead ? h('header', { className: 'fl-card-head' },
        h('div', null,
          props.eyebrow ? h('span', { className: 'fl-card-eyebrow' }, props.eyebrow) : null,
          props.title ? h('h3', { className: 'fl-card-title' }, props.title) : null
        ),
        props.meta ? h('div', null, props.meta) : null
      ) : null,
      props.children !== undefined && props.children !== null ? h('div', { className: 'fl-card-body' }, props.children) : null,
      props.footer ? h('footer', { className: 'fl-card-foot' }, props.footer) : null
    );
  }

  /* Tabs — a tab strip. Controlled with value/onChange, or uncontrolled with defaultValue. */
  function Tabs(props) {
    var items = props.items || [];
    var first = items.length ? items[0].id : undefined;
    var state = React.useState(props.defaultValue !== undefined ? props.defaultValue : first);
    var current = props.value !== undefined ? props.value : state[0];
    function select(id) {
      if (props.value === undefined) state[1](id);
      if (props.onChange) props.onChange(id);
    }
    function onKeyDown(e) {
      if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
      var enabled = items.filter(function (it) { return !it.disabled; });
      var i = enabled.findIndex(function (it) { return it.id === current; });
      var next = enabled[(i + (e.key === 'ArrowRight' ? 1 : enabled.length - 1)) % enabled.length];
      if (!next) return;
      e.preventDefault();
      select(next.id);
      var btn = e.currentTarget.querySelector('[data-tab="' + next.id + '"]');
      if (btn) btn.focus();
    }
    return h('div', { className: cx('fl-tabs', props.className), role: 'tablist', 'aria-label': props.label, onKeyDown: onKeyDown },
      items.map(function (it) {
        var on = it.id === current;
        return h('button', {
          key: it.id, type: 'button', role: 'tab', className: 'fl-tab', 'data-tab': it.id,
          'aria-selected': on ? 'true' : 'false', tabIndex: on ? 0 : -1, disabled: it.disabled,
          onClick: function () { select(it.id); }
        }, it.label, it.count !== undefined ? h('span', { className: 'fl-tab-count' }, it.count) : null);
      })
    );
  }

  /* Menu — the overlay list: items, separators and group labels. Arrow keys move between items. */
  function Menu(props) {
    var items = props.items || [];
    function onKeyDown(e) {
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
      var list = Array.prototype.slice.call(e.currentTarget.querySelectorAll('.fl-menu-item:not(:disabled)'));
      if (!list.length) return;
      e.preventDefault();
      var i = list.indexOf(document.activeElement);
      var n = e.key === 'ArrowDown' ? (i + 1) % list.length : (i <= 0 ? list.length - 1 : i - 1);
      list[n].focus();
    }
    return h('div', { className: cx('fl-menu', props.className), role: 'menu', 'aria-label': props.label, onKeyDown: onKeyDown, style: props.style },
      items.map(function (it, i) {
        if (it.type === 'separator') return h('div', { key: 's' + i, className: 'fl-menu-sep', role: 'separator' });
        if (it.type === 'label') return h('div', { key: 'l' + i, className: 'fl-menu-label', role: 'presentation' }, it.label);
        return h('button', {
          key: 'i' + i, type: 'button', role: 'menuitem', disabled: it.disabled,
          className: cx('fl-menu-item', it.tone === 'danger' && 'fl-menu-item-danger'),
          onClick: function () { if (it.onSelect) it.onSelect(); if (props.onSelect) props.onSelect(it); }
        }, h('span', null, it.label), it.shortcut ? h('span', { className: 'fl-menu-key' }, it.shortcut) : null);
      })
    );
  }

  /* BarChart — vertical or horizontal, grouped or stacked bars in the fixed chart-1…chart-5 order.
     One series draws in ink-muted with no legend. Hover or focus a category for every value in it. */
  var CHART_SLOTS = 5;
  var chartNumber = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 });
  function chartFormat(v) { return chartNumber.format(v); }
  function niceScale(max) {
    if (!(max > 0)) return { top: 1, ticks: [0, 1] };
    var raw = max / 4;
    var mag = Math.pow(10, Math.floor(Math.log(raw) / Math.LN10));
    var mults = [1, 2, 2.5, 5, 10], step = mag * 10;
    for (var i = 0; i < mults.length; i++) { if (mag * mults[i] >= raw) { step = mag * mults[i]; break; } }
    var top = Math.ceil(max / step - 1e-9) * step;
    var ticks = [];
    for (var n = 0; n * step <= top + step / 2; n++) ticks.push(Math.round(n * step * 1e6) / 1e6);
    return { top: top, ticks: ticks };
  }

  function BarChart(props) {
    var categories = props.categories || [];
    var all = props.series || [];
    if (all.length > CHART_SLOTS && window.console) console.warn('Flint BarChart: at most ' + CHART_SLOTS + ' series — fold the rest into one { slot: "other" } series.');
    var series = all.slice(0, CHART_SLOTS);
    var horizontal = props.orientation === 'horizontal';
    var single = series.length === 1;
    var stacked = !!props.stacked && !single;
    var fmt = props.formatValue || chartFormat;
    var showLegend = props.legend !== undefined ? props.legend : !single;
    var plotRef = React.useRef(null);
    var tipState = React.useState(null), tip = tipState[0], setTip = tipState[1];

    function val(s, i) { var v = Number(s.values && s.values[i]); return v > 0 ? v : 0; }
    function colour(s, k) { return 'fl-chart-c' + (single ? '0' : (s.slot === 'other' ? 'x' : (s.slot || k + 1))); }
    function total(i) { var t = 0; series.forEach(function (s) { t += val(s, i); }); return t; }
    var max = 0;
    categories.forEach(function (_, i) {
      if (stacked) max = Math.max(max, total(i));
      else series.forEach(function (s) { max = Math.max(max, val(s, i)); });
    });
    var scale = niceScale(props.max || max);
    function pct(v) { return Math.min(100, (v / scale.top) * 100) + '%'; }
    function describe(i) {
      return categories[i] + ': ' + series.map(function (s) { return s.name + ' ' + fmt(val(s, i)); }).join(', ') +
        (stacked ? ', total ' + fmt(total(i)) : '');
    }
    function at(i, x, y) {
      var r = plotRef.current.getBoundingClientRect();
      setTip({ i: i, x: x - r.left, y: y - r.top, w: r.width, h: r.height });
    }
    function onMove(i) { return function (e) { at(i, e.clientX, e.clientY); }; }
    function onFocus(i) { return function (e) { var b = e.currentTarget.getBoundingClientRect(); at(i, b.left + b.width / 2, b.top + b.height / 2); }; }
    function clear() { setTip(null); }

    function marks(i) {
      var size = horizontal ? 'width' : 'height';
      if (stacked) {
        var st = {}; st[size] = pct(total(i));
        return h('div', { className: 'fl-chart-stack', style: st },
          series.map(function (s, k) {
            var v = val(s, i);
            return v > 0 ? h('div', { key: k, className: 'fl-chart-seg ' + colour(s, k), style: { flexGrow: v } }) : null;
          }));
      }
      return series.map(function (s, k) {
        var st = {}; st[size] = pct(val(s, i));
        return h('div', { key: k, className: 'fl-chart-bar ' + colour(s, k), style: st });
      });
    }
    function band(i, style) {
      return h('div', {
        key: i, className: cx('fl-chart-band', tip && tip.i === i && 'is-on'), style: style, tabIndex: 0,
        'aria-label': describe(i), onPointerMove: onMove(i), onFocus: onFocus(i), onBlur: clear
      }, h('div', { className: 'fl-chart-marks' }, marks(i)));
    }
    var tooltip = tip && categories[tip.i] !== undefined ? h('div', {
      className: 'fl-chart-tip', 'aria-hidden': 'true',
      style: {
        left: tip.x, top: tip.y,
        transform: 'translate(' + (tip.x > tip.w / 2 ? 'calc(-100% - 12px)' : '12px') + ', ' + (tip.y > tip.h / 2 ? 'calc(-100% - 12px)' : '12px') + ')'
      }
    },
      h('div', { className: 'fl-chart-tip-head' }, categories[tip.i]),
      series.map(function (s, k) {
        return h('div', { key: k, className: 'fl-chart-tip-row' },
          h('span', { className: 'fl-chart-key ' + colour(s, k) }),
          h('span', { className: 'fl-chart-tip-val' }, fmt(val(s, tip.i))),
          h('span', { className: 'fl-chart-tip-name' }, s.name));
      }),
      stacked ? h('div', { className: 'fl-chart-tip-row fl-chart-tip-total' },
        h('span', null), h('span', { className: 'fl-chart-tip-val' }, fmt(total(tip.i))), h('span', { className: 'fl-chart-tip-name' }, 'Total')) : null
    ) : null;

    var longest = scale.ticks.reduce(function (a, t) { var s = fmt(t); return s.length > a.length ? s : a; }, '');
    var body;
    if (!horizontal) {
      body = h('div', { className: 'fl-chart-v', style: { '--fl-chart-h': (props.height || 200) + 'px' } },
        h('div', { className: 'fl-chart-y', 'aria-hidden': 'true' },
          h('span', { className: 'fl-chart-sizer' }, longest),
          scale.ticks.map(function (t) { return h('span', { key: t, className: 'fl-chart-tick', style: { bottom: pct(t) } }, fmt(t)); })),
        h('div', { className: 'fl-chart-plot', ref: plotRef, onPointerLeave: clear },
          scale.ticks.map(function (t) { return t > 0 ? h('div', { key: t, className: 'fl-chart-grid', style: { bottom: pct(t) } }) : null; }),
          h('div', { className: 'fl-chart-bands' }, categories.map(function (_, i) { return band(i); })),
          tooltip),
        h('div', null),
        h('div', { className: 'fl-chart-x', 'aria-hidden': 'true' }, categories.map(function (c, i) { return h('span', { key: i }, c); }))
      );
    } else {
      var thick = stacked || single ? 16 : 12;
      var rowH = (stacked || single ? thick : series.length * thick + (series.length - 1) * 2) + 16;
      body = h('div', { className: 'fl-chart-hz' },
        h('div', { className: 'fl-chart-cats', 'aria-hidden': 'true' }, categories.map(function (c, i) { return h('span', { key: i, style: { height: rowH } }, c); })),
        h('div', { className: 'fl-chart-plot', ref: plotRef, onPointerLeave: clear },
          scale.ticks.map(function (t) { return t > 0 ? h('div', { key: t, className: 'fl-chart-grid', style: { left: pct(t) } }) : null; }),
          h('div', { className: 'fl-chart-rows', style: { '--fl-chart-bar': thick + 'px' } }, categories.map(function (_, i) { return band(i, { height: rowH }); })),
          tooltip),
        h('div', null),
        h('div', { className: 'fl-chart-xh', 'aria-hidden': 'true' },
          scale.ticks.map(function (t, n) {
            var edge = n === 0 ? 'none' : (n === scale.ticks.length - 1 ? 'translateX(-100%)' : 'translateX(-50%)');
            return h('span', { key: t, className: 'fl-chart-tick', style: { left: pct(t), transform: edge } }, fmt(t));
          }))
      );
    }

    return h('figure', {
      className: cx('fl-chart', horizontal ? 'is-horizontal' : 'is-vertical', single && 'is-single', stacked && 'is-stacked', props.className),
      style: props.style, 'aria-label': props.label
    },
      showLegend ? h('div', { className: 'fl-chart-legend' }, series.map(function (s, k) {
        return h('span', { key: k, className: 'fl-chart-legend-item' }, h('span', { className: 'fl-chart-sw ' + colour(s, k) }), s.name);
      })) : null,
      body,
      h('table', { className: 'fl-chart-sr' },
        props.label ? h('caption', null, props.label) : null,
        h('thead', null, h('tr', null, h('th', { scope: 'col' }, props.categoryLabel || 'Category'),
          series.map(function (s, k) { return h('th', { key: k, scope: 'col' }, s.name); }),
          stacked ? h('th', { scope: 'col' }, 'Total') : null)),
        h('tbody', null, categories.map(function (c, i) {
          return h('tr', { key: i }, h('th', { scope: 'row' }, c),
            series.map(function (s, k) { return h('td', { key: k }, fmt(val(s, i))); }),
            stacked ? h('td', null, fmt(total(i))) : null);
        })))
    );
  }

  var api = { Button: Button, Input: Input, Tag: Tag, Card: Card, Tabs: Tabs, Menu: Menu, BarChart: BarChart };
  window.Flint = window.Flint || {};
  for (var k in api) window.Flint[k] = api[k];
})();
