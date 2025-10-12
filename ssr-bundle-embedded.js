
// ============ SSR Bundle Start ============
var SSRBundle = function(exports) {
  "use strict";
  var n$1, l$1, u$1, v$1 = [];
  function _(l2, u2, t) {
    var i2, r, o2, e = {};
    for (o2 in u2) "key" == o2 ? i2 = u2[o2] : "ref" == o2 ? r = u2[o2] : e[o2] = u2[o2];
    if (arguments.length > 2 && (e.children = arguments.length > 3 ? n$1.call(arguments, 2) : t), "function" == typeof l2 && null != l2.defaultProps) for (o2 in l2.defaultProps) void 0 === e[o2] && (e[o2] = l2.defaultProps[o2]);
    return m(l2, e, i2, r, null);
  }
  function m(n2, t, i2, r, o2) {
    var e = { type: n2, props: t, key: i2, ref: r, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: null == o2 ? ++u$1 : o2, __i: -1, __u: 0 };
    return null != l$1.vnode && l$1.vnode(e), e;
  }
  function k$1(n2) {
    return n2.children;
  }
  n$1 = v$1.slice, l$1 = { __e: function(n2, l2, u2, t) {
    for (var i2, r, o2; l2 = l2.__; ) if ((i2 = l2.__c) && !i2.__) try {
      if ((r = i2.constructor) && null != r.getDerivedStateFromError && (i2.setState(r.getDerivedStateFromError(n2)), o2 = i2.__d), null != i2.componentDidCatch && (i2.componentDidCatch(n2, t || {}), o2 = i2.__d), o2) return i2.__E = i2;
    } catch (l3) {
      n2 = l3;
    }
    throw n2;
  } }, u$1 = 0, "function" == typeof Promise ? Promise.prototype.then.bind(Promise.resolve()) : setTimeout;
  var n = /[\s\n\\/='"\0<>]/, o = /^(xlink|xmlns|xml)([A-Z])/, i = /^(?:accessK|auto[A-Z]|cell|ch|col|cont|cross|dateT|encT|form[A-Z]|frame|hrefL|inputM|maxL|minL|noV|playsI|popoverT|readO|rowS|src[A-Z]|tabI|useM|item[A-Z])/, a = /^ac|^ali|arabic|basel|cap|clipPath$|clipRule$|color|dominant|enable|fill|flood|font|glyph[^R]|horiz|image|letter|lighting|marker[^WUH]|overline|panose|pointe|paint|rendering|shape|stop|strikethrough|stroke|text[^L]|transform|underline|unicode|units|^v[^i]|^w|^xH/, c = /* @__PURE__ */ new Set(["draggable", "spellcheck"]), s = /["&<]/;
  function l(e) {
    if (0 === e.length || false === s.test(e)) return e;
    for (var t = 0, r = 0, n2 = "", o2 = ""; r < e.length; r++) {
      switch (e.charCodeAt(r)) {
        case 34:
          o2 = "&quot;";
          break;
        case 38:
          o2 = "&amp;";
          break;
        case 60:
          o2 = "&lt;";
          break;
        default:
          continue;
      }
      r !== t && (n2 += e.slice(t, r)), n2 += o2, t = r + 1;
    }
    return r !== t && (n2 += e.slice(t, r)), n2;
  }
  var u = {}, f = /* @__PURE__ */ new Set(["animation-iteration-count", "border-image-outset", "border-image-slice", "border-image-width", "box-flex", "box-flex-group", "box-ordinal-group", "column-count", "fill-opacity", "flex", "flex-grow", "flex-negative", "flex-order", "flex-positive", "flex-shrink", "flood-opacity", "font-weight", "grid-column", "grid-row", "line-clamp", "line-height", "opacity", "order", "orphans", "stop-opacity", "stroke-dasharray", "stroke-dashoffset", "stroke-miterlimit", "stroke-opacity", "stroke-width", "tab-size", "widows", "z-index", "zoom"]), p = /[A-Z]/g;
  function h(e) {
    var t = "";
    for (var r in e) {
      var n2 = e[r];
      if (null != n2 && "" !== n2) {
        var o2 = "-" == r[0] ? r : u[r] || (u[r] = r.replace(p, "-$&").toLowerCase()), i2 = ";";
        "number" != typeof n2 || o2.startsWith("--") || f.has(o2) || (i2 = "px;"), t = t + o2 + ":" + n2 + i2;
      }
    }
    return t || void 0;
  }
  function d() {
    this.__d = true;
  }
  function v(e, t) {
    return { __v: e, context: t, props: e.props, setState: d, forceUpdate: d, __d: true, __h: new Array(0) };
  }
  var k, w, x, C, S = {}, L = [], E = Array.isArray, T = Object.assign, j = "";
  function D(n2, o2, i2) {
    var a2 = l$1.__s;
    l$1.__s = true, k = l$1.__b, w = l$1.diffed, x = l$1.__r, C = l$1.unmount;
    var c2 = _(k$1, null);
    c2.__k = [n2];
    try {
      var s2 = U(n2, o2 || S, false, void 0, c2, false, i2);
      return E(s2) ? s2.join(j) : s2;
    } catch (e) {
      if (e.then) throw new Error('Use "renderToStringAsync" for suspenseful rendering.');
      throw e;
    } finally {
      l$1.__c && l$1.__c(n2, L), l$1.__s = a2, L.length = 0;
    }
  }
  function P(e, t) {
    var r, n2 = e.type, o2 = true;
    return e.__c ? (o2 = false, (r = e.__c).state = r.__s) : r = new n2(e.props, t), e.__c = r, r.__v = e, r.props = e.props, r.context = t, r.__d = true, null == r.state && (r.state = S), null == r.__s && (r.__s = r.state), n2.getDerivedStateFromProps ? r.state = T({}, r.state, n2.getDerivedStateFromProps(r.props, r.state)) : o2 && r.componentWillMount ? (r.componentWillMount(), r.state = r.__s !== r.state ? r.__s : r.state) : !o2 && r.componentWillUpdate && r.componentWillUpdate(), x && x(e), r.render(r.props, r.state, t);
  }
  function U(t, s2, u2, f2, p2, d2, _2) {
    if (null == t || true === t || false === t || t === j) return j;
    var m2 = typeof t;
    if ("object" != m2) return "function" == m2 ? j : "string" == m2 ? l(t) : t + j;
    if (E(t)) {
      var y, g = j;
      p2.__k = t;
      for (var b = t.length, A = 0; A < b; A++) {
        var L2 = t[A];
        if (null != L2 && "boolean" != typeof L2) {
          var D2, F2 = U(L2, s2, u2, f2, p2, d2, _2);
          "string" == typeof F2 ? g += F2 : (y || (y = new Array(b)), g && y.push(g), g = j, E(F2) ? (D2 = y).push.apply(D2, F2) : y.push(F2));
        }
      }
      return y ? (g && y.push(g), y) : g;
    }
    if (void 0 !== t.constructor) return j;
    t.__ = p2, k && k(t);
    var M = t.type, W = t.props;
    if ("function" == typeof M) {
      var $, z, H, N = s2;
      if (M === k$1) {
        if ("tpl" in W) {
          for (var q = j, B = 0; B < W.tpl.length; B++) if (q += W.tpl[B], W.exprs && B < W.exprs.length) {
            var I = W.exprs[B];
            if (null == I) continue;
            "object" != typeof I || void 0 !== I.constructor && !E(I) ? q += I : q += U(I, s2, u2, f2, t, d2, _2);
          }
          return q;
        }
        if ("UNSTABLE_comment" in W) return "<!--" + l(W.UNSTABLE_comment) + "-->";
        z = W.children;
      } else {
        if (null != ($ = M.contextType)) {
          var O = s2[$.__c];
          N = O ? O.props.value : $.__;
        }
        var R = M.prototype && "function" == typeof M.prototype.render;
        if (R) z = P(t, N), H = t.__c;
        else {
          t.__c = H = v(t, N);
          for (var V = 0; H.__d && V++ < 25; ) H.__d = false, x && x(t), z = M.call(H, W, N);
          H.__d = true;
        }
        if (null != H.getChildContext && (s2 = T({}, s2, H.getChildContext())), R && l$1.errorBoundaries && (M.getDerivedStateFromError || H.componentDidCatch)) {
          z = null != z && z.type === k$1 && null == z.key && null == z.props.tpl ? z.props.children : z;
          try {
            return U(z, s2, u2, f2, t, d2, _2);
          } catch (e) {
            return M.getDerivedStateFromError && (H.__s = M.getDerivedStateFromError(e)), H.componentDidCatch && H.componentDidCatch(e, S), H.__d ? (z = P(t, s2), null != (H = t.__c).getChildContext && (s2 = T({}, s2, H.getChildContext())), U(z = null != z && z.type === k$1 && null == z.key && null == z.props.tpl ? z.props.children : z, s2, u2, f2, t, d2, _2)) : j;
          } finally {
            w && w(t), C && C(t);
          }
        }
      }
      z = null != z && z.type === k$1 && null == z.key && null == z.props.tpl ? z.props.children : z;
      try {
        var K = U(z, s2, u2, f2, t, d2, _2);
        return w && w(t), l$1.unmount && l$1.unmount(t), K;
      } catch (r) {
        if (_2 && _2.onError) {
          var G = _2.onError(r, t, function(e, t2) {
            return U(e, s2, u2, f2, t2, d2, _2);
          });
          if (void 0 !== G) return G;
          var J = l$1.__e;
          return J && J(r, t), j;
        }
        throw r;
      }
    }
    var Q, X = "<" + M, Y = j;
    for (var ee in W) {
      var te = W[ee];
      if ("function" != typeof te || "class" === ee || "className" === ee) {
        switch (ee) {
          case "children":
            Q = te;
            continue;
          case "key":
          case "ref":
          case "__self":
          case "__source":
            continue;
          case "htmlFor":
            if ("for" in W) continue;
            ee = "for";
            break;
          case "className":
            if ("class" in W) continue;
            ee = "class";
            break;
          case "defaultChecked":
            ee = "checked";
            break;
          case "defaultSelected":
            ee = "selected";
            break;
          case "defaultValue":
          case "value":
            switch (ee = "value", M) {
              case "textarea":
                Q = te;
                continue;
              case "select":
                f2 = te;
                continue;
              case "option":
                f2 != te || "selected" in W || (X += " selected");
            }
            break;
          case "dangerouslySetInnerHTML":
            Y = te && te.__html;
            continue;
          case "style":
            "object" == typeof te && (te = h(te));
            break;
          case "acceptCharset":
            ee = "accept-charset";
            break;
          case "httpEquiv":
            ee = "http-equiv";
            break;
          default:
            if (o.test(ee)) ee = ee.replace(o, "$1:$2").toLowerCase();
            else {
              if (n.test(ee)) continue;
              "-" !== ee[4] && !c.has(ee) || null == te ? u2 ? a.test(ee) && (ee = "panose1" === ee ? "panose-1" : ee.replace(/([A-Z])/g, "-$1").toLowerCase()) : i.test(ee) && (ee = ee.toLowerCase()) : te += j;
            }
        }
        null != te && false !== te && (X = true === te || te === j ? X + " " + ee : X + " " + ee + '="' + ("string" == typeof te ? l(te) : te + j) + '"');
      }
    }
    if (n.test(M)) throw new Error(M + " is not a valid HTML tag name in " + X + ">");
    if (Y || ("string" == typeof Q ? Y = l(Q) : null != Q && false !== Q && true !== Q && (Y = U(Q, s2, "svg" === M || "foreignObject" !== M && u2, f2, t, d2, _2))), w && w(t), C && C(t), !Y && Z.has(M)) return X + "/>";
    var re = "</" + M + ">", ne = X + ">";
    return E(Y) ? [ne].concat(Y, [re]) : "string" != typeof Y ? [ne, Y, re] : ne + Y + re;
  }
  var Z = /* @__PURE__ */ new Set(["area", "base", "br", "col", "command", "embed", "hr", "img", "input", "keygen", "link", "meta", "param", "source", "track", "wbr"]), F = D;
  function SimpleApp({ products = [] }) {
    return _(
      "div",
      { className: "container" },
      _(
        "header",
        { className: "shop-header" },
        _("h1", { className: "title" }, "🛍️ Магазин Enddel"),
        _(
          "div",
          { className: "shop-controls" },
          _(
            "div",
            { className: "search-container" },
            _("input", {
              type: "text",
              className: "input input-search",
              placeholder: "Поиск товаров...",
              disabled: true
            }),
            _("span", { className: "search-emoji" }, "🔍")
          ),
          _(
            "div",
            { className: "cart-indicator" },
            _("span", null, "🛒"),
            " 0 товаров | 0 ₽"
          )
        )
      ),
      _(
        "div",
        { className: "products-grid" },
        products.length > 0 ? products.map(
          (product) => {
            var _a, _b;
            return _(
              "div",
              { key: product.id, className: "product-card" },
              _(
                "div",
                { className: "product-image" },
                product.image_url ? _("img", {
                  src: product.image_url,
                  alt: ((_a = product.name) == null ? void 0 : _a.ru) || "Товар",
                  loading: "lazy"
                }) : _("div", { className: "product-placeholder" }, "📦")
              ),
              _(
                "h3",
                { className: "product-name" },
                ((_b = product.name) == null ? void 0 : _b.ru) || product.name || "Товар"
              ),
              _(
                "div",
                { className: "product-price" },
                `${product.price || 0} ₽`
              ),
              _("button", {
                className: "btn btn-primary",
                disabled: true
              }, "В корзину")
            );
          }
        ) : _("div", { className: "empty-state" }, "Загрузка товаров...")
      )
    );
  }
  async function renderToString(context) {
    let initialData = {
      products: [],
      url: context.url,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    try {
      const response = await fetch("https://enddel.com/products", {
        headers: {
          "User-Agent": "Mozilla/5.0 (compatible; EndelSSR/1.0)"
        }
      });
      const data = await response.json();
      initialData.products = (data.products || data || []).slice(0, 20);
      console.log(`✅ Loaded ${initialData.products.length} products for SSR`);
      console.log("📦 Sample product:", initialData.products[0] ? JSON.stringify(initialData.products[0], null, 2) : "No products");
    } catch (error) {
      console.error("Failed to load products:", error);
      initialData.products = [{
        id: 1,
        name: { ru: "Тестовый товар", en: "Test Product", geo: "სატესტო პროდუქტი" },
        price: 100,
        image_url: null,
        unit: "шт",
        step: 1,
        stock_quantity: 10
      }];
      console.log("🧪 Using test product for debugging");
    }
    const html = F(_(SimpleApp, { products: initialData.products }));
    return {
      html,
      head: "",
      initialData
    };
  }
  exports.renderToString = renderToString;
  Object.defineProperty(exports, Symbol.toStringTag, { value: "Module" });
  return exports;
}({});

// ============ SSR Bundle End ============

// Экспортируем renderToString глобально для Rust
globalThis.renderToString = SSRBundle.renderToString;

// Глобальная функция для рендеринга (вызывается из Rust)
globalThis.renderPage = async function(url) {
    try {
        // Вызываем рендеринг
        const context = {
            url: url,
            headers: {},
            userAgent: 'Rust-V8-SSR/1.0'
        };

        const result = await SSRBundle.renderToString(context);

        // Формируем полный HTML
        const html = `<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Enddel - Магазин</title>
    <link rel="stylesheet" href="/assets/index-CXxKVYtV.css">
    ${result.head || ''}
</head>
<body>
    <div id="app" data-preact-root>${result.html}</div>
    <script>window.__INITIAL_DATA__ = ${JSON.stringify(result.initialData)}</script>
    <script type="module" src="/assets/index-DdBg9HLV.js"></script>
</body>
</html>`;

        return html;
    } catch (error) {
        console.error('SSR Error:', error);
        throw error;
    }
};

console.log('✅ SSR bundle loaded and ready');
