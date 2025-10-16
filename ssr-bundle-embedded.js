
// ============ SSR Bundle Start ============
var SSRBundle = function(exports) {
  "use strict";
  var n$2, l$3, u$4, t$2, i$3, r$2, o$2, e$2, f$4, c$4, s$3, a$3, p$4 = {}, v$4 = [], y$2 = /acit|ex(?:s|g|n|p|$)|rph|grid|ows|mnc|ntw|ine[ch]|zoo|^ord|itera/i, w$3 = Array.isArray;
  function d$4(n2, l2) {
    for (var u2 in l2) n2[u2] = l2[u2];
    return n2;
  }
  function g$2(n2) {
    n2 && n2.parentNode && n2.parentNode.removeChild(n2);
  }
  function _$1(l2, u2, t2) {
    var i2, r2, o2, e2 = {};
    for (o2 in u2) "key" == o2 ? i2 = u2[o2] : "ref" == o2 ? r2 = u2[o2] : e2[o2] = u2[o2];
    if (arguments.length > 2 && (e2.children = arguments.length > 3 ? n$2.call(arguments, 2) : t2), "function" == typeof l2 && null != l2.defaultProps) for (o2 in l2.defaultProps) void 0 === e2[o2] && (e2[o2] = l2.defaultProps[o2]);
    return m$1(l2, e2, i2, r2, null);
  }
  function m$1(n2, t2, i2, r2, o2) {
    var e2 = { type: n2, props: t2, key: i2, ref: r2, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: null == o2 ? ++u$4 : o2, __i: -1, __u: 0 };
    return null == o2 && null != l$3.vnode && l$3.vnode(e2), e2;
  }
  function k$2(n2) {
    return n2.children;
  }
  function x$1(n2, l2) {
    this.props = n2, this.context = l2;
  }
  function S$1(n2, l2) {
    if (null == l2) return n2.__ ? S$1(n2.__, n2.__i + 1) : null;
    for (var u2; l2 < n2.__k.length; l2++) if (null != (u2 = n2.__k[l2]) && null != u2.__e) return u2.__e;
    return "function" == typeof n2.type ? S$1(n2) : null;
  }
  function C$2(n2) {
    var l2, u2;
    if (null != (n2 = n2.__) && null != n2.__c) {
      for (n2.__e = n2.__c.base = null, l2 = 0; l2 < n2.__k.length; l2++) if (null != (u2 = n2.__k[l2]) && null != u2.__e) {
        n2.__e = n2.__c.base = u2.__e;
        break;
      }
      return C$2(n2);
    }
  }
  function M$1(n2) {
    (!n2.__d && (n2.__d = true) && i$3.push(n2) && !$.__r++ || r$2 != l$3.debounceRendering) && ((r$2 = l$3.debounceRendering) || o$2)($);
  }
  function $() {
    for (var n2, u2, t2, r2, o2, f2, c2, s2 = 1; i$3.length; ) i$3.length > s2 && i$3.sort(e$2), n2 = i$3.shift(), s2 = i$3.length, n2.__d && (t2 = void 0, o2 = (r2 = (u2 = n2).__v).__e, f2 = [], c2 = [], u2.__P && ((t2 = d$4({}, r2)).__v = r2.__v + 1, l$3.vnode && l$3.vnode(t2), O(u2.__P, t2, r2, u2.__n, u2.__P.namespaceURI, 32 & r2.__u ? [o2] : null, f2, null == o2 ? S$1(r2) : o2, !!(32 & r2.__u), c2), t2.__v = r2.__v, t2.__.__k[t2.__i] = t2, z$1(f2, t2, c2), t2.__e != o2 && C$2(t2)));
    $.__r = 0;
  }
  function I(n2, l2, u2, t2, i2, r2, o2, e2, f2, c2, s2) {
    var a2, h2, y2, w2, d2, g2, _2 = t2 && t2.__k || v$4, m2 = l2.length;
    for (f2 = P$2(u2, l2, _2, f2, m2), a2 = 0; a2 < m2; a2++) null != (y2 = u2.__k[a2]) && (h2 = -1 == y2.__i ? p$4 : _2[y2.__i] || p$4, y2.__i = a2, g2 = O(n2, y2, h2, i2, r2, o2, e2, f2, c2, s2), w2 = y2.__e, y2.ref && h2.ref != y2.ref && (h2.ref && q$2(h2.ref, null, y2), s2.push(y2.ref, y2.__c || w2, y2)), null == d2 && null != w2 && (d2 = w2), 4 & y2.__u || h2.__k === y2.__k ? f2 = A$1(y2, f2, n2) : "function" == typeof y2.type && void 0 !== g2 ? f2 = g2 : w2 && (f2 = w2.nextSibling), y2.__u &= -7);
    return u2.__e = d2, f2;
  }
  function P$2(n2, l2, u2, t2, i2) {
    var r2, o2, e2, f2, c2, s2 = u2.length, a2 = s2, h2 = 0;
    for (n2.__k = new Array(i2), r2 = 0; r2 < i2; r2++) null != (o2 = l2[r2]) && "boolean" != typeof o2 && "function" != typeof o2 ? (f2 = r2 + h2, (o2 = n2.__k[r2] = "string" == typeof o2 || "number" == typeof o2 || "bigint" == typeof o2 || o2.constructor == String ? m$1(null, o2, null, null, null) : w$3(o2) ? m$1(k$2, { children: o2 }, null, null, null) : null == o2.constructor && o2.__b > 0 ? m$1(o2.type, o2.props, o2.key, o2.ref ? o2.ref : null, o2.__v) : o2).__ = n2, o2.__b = n2.__b + 1, e2 = null, -1 != (c2 = o2.__i = L$1(o2, u2, f2, a2)) && (a2--, (e2 = u2[c2]) && (e2.__u |= 2)), null == e2 || null == e2.__v ? (-1 == c2 && (i2 > s2 ? h2-- : i2 < s2 && h2++), "function" != typeof o2.type && (o2.__u |= 4)) : c2 != f2 && (c2 == f2 - 1 ? h2-- : c2 == f2 + 1 ? h2++ : (c2 > f2 ? h2-- : h2++, o2.__u |= 4))) : n2.__k[r2] = null;
    if (a2) for (r2 = 0; r2 < s2; r2++) null != (e2 = u2[r2]) && 0 == (2 & e2.__u) && (e2.__e == t2 && (t2 = S$1(e2)), B$2(e2, e2));
    return t2;
  }
  function A$1(n2, l2, u2) {
    var t2, i2;
    if ("function" == typeof n2.type) {
      for (t2 = n2.__k, i2 = 0; t2 && i2 < t2.length; i2++) t2[i2] && (t2[i2].__ = n2, l2 = A$1(t2[i2], l2, u2));
      return l2;
    }
    n2.__e != l2 && (l2 && n2.type && !u2.contains(l2) && (l2 = S$1(n2)), u2.insertBefore(n2.__e, l2 || null), l2 = n2.__e);
    do {
      l2 = l2 && l2.nextSibling;
    } while (null != l2 && 8 == l2.nodeType);
    return l2;
  }
  function H$1(n2, l2) {
    return l2 = l2 || [], null == n2 || "boolean" == typeof n2 || (w$3(n2) ? n2.some(function(n3) {
      H$1(n3, l2);
    }) : l2.push(n2)), l2;
  }
  function L$1(n2, l2, u2, t2) {
    var i2, r2, o2 = n2.key, e2 = n2.type, f2 = l2[u2];
    if (null === f2 && null == n2.key || f2 && o2 == f2.key && e2 == f2.type && 0 == (2 & f2.__u)) return u2;
    if (t2 > (null != f2 && 0 == (2 & f2.__u) ? 1 : 0)) for (i2 = u2 - 1, r2 = u2 + 1; i2 >= 0 || r2 < l2.length; ) {
      if (i2 >= 0) {
        if ((f2 = l2[i2]) && 0 == (2 & f2.__u) && o2 == f2.key && e2 == f2.type) return i2;
        i2--;
      }
      if (r2 < l2.length) {
        if ((f2 = l2[r2]) && 0 == (2 & f2.__u) && o2 == f2.key && e2 == f2.type) return r2;
        r2++;
      }
    }
    return -1;
  }
  function T$3(n2, l2, u2) {
    "-" == l2[0] ? n2.setProperty(l2, null == u2 ? "" : u2) : n2[l2] = null == u2 ? "" : "number" != typeof u2 || y$2.test(l2) ? u2 : u2 + "px";
  }
  function j$3(n2, l2, u2, t2, i2) {
    var r2, o2;
    n: if ("style" == l2) if ("string" == typeof u2) n2.style.cssText = u2;
    else {
      if ("string" == typeof t2 && (n2.style.cssText = t2 = ""), t2) for (l2 in t2) u2 && l2 in u2 || T$3(n2.style, l2, "");
      if (u2) for (l2 in u2) t2 && u2[l2] == t2[l2] || T$3(n2.style, l2, u2[l2]);
    }
    else if ("o" == l2[0] && "n" == l2[1]) r2 = l2 != (l2 = l2.replace(f$4, "$1")), o2 = l2.toLowerCase(), l2 = o2 in n2 || "onFocusOut" == l2 || "onFocusIn" == l2 ? o2.slice(2) : l2.slice(2), n2.l || (n2.l = {}), n2.l[l2 + r2] = u2, u2 ? t2 ? u2.u = t2.u : (u2.u = c$4, n2.addEventListener(l2, r2 ? a$3 : s$3, r2)) : n2.removeEventListener(l2, r2 ? a$3 : s$3, r2);
    else {
      if ("http://www.w3.org/2000/svg" == i2) l2 = l2.replace(/xlink(H|:h)/, "h").replace(/sName$/, "s");
      else if ("width" != l2 && "height" != l2 && "href" != l2 && "list" != l2 && "form" != l2 && "tabIndex" != l2 && "download" != l2 && "rowSpan" != l2 && "colSpan" != l2 && "role" != l2 && "popover" != l2 && l2 in n2) try {
        n2[l2] = null == u2 ? "" : u2;
        break n;
      } catch (n3) {
      }
      "function" == typeof u2 || (null == u2 || false === u2 && "-" != l2[4] ? n2.removeAttribute(l2) : n2.setAttribute(l2, "popover" == l2 && 1 == u2 ? "" : u2));
    }
  }
  function F$2(n2) {
    return function(u2) {
      if (this.l) {
        var t2 = this.l[u2.type + n2];
        if (null == u2.t) u2.t = c$4++;
        else if (u2.t < t2.u) return;
        return t2(l$3.event ? l$3.event(u2) : u2);
      }
    };
  }
  function O(n2, u2, t2, i2, r2, o2, e2, f2, c2, s2) {
    var a2, h2, p2, v2, y2, _2, m2, b2, S2, C2, M2, $2, P2, A2, H2, L2, T2, j2 = u2.type;
    if (null != u2.constructor) return null;
    128 & t2.__u && (c2 = !!(32 & t2.__u), o2 = [f2 = u2.__e = t2.__e]), (a2 = l$3.__b) && a2(u2);
    n: if ("function" == typeof j2) try {
      if (b2 = u2.props, S2 = "prototype" in j2 && j2.prototype.render, C2 = (a2 = j2.contextType) && i2[a2.__c], M2 = a2 ? C2 ? C2.props.value : a2.__ : i2, t2.__c ? m2 = (h2 = u2.__c = t2.__c).__ = h2.__E : (S2 ? u2.__c = h2 = new j2(b2, M2) : (u2.__c = h2 = new x$1(b2, M2), h2.constructor = j2, h2.render = D$2), C2 && C2.sub(h2), h2.props = b2, h2.state || (h2.state = {}), h2.context = M2, h2.__n = i2, p2 = h2.__d = true, h2.__h = [], h2._sb = []), S2 && null == h2.__s && (h2.__s = h2.state), S2 && null != j2.getDerivedStateFromProps && (h2.__s == h2.state && (h2.__s = d$4({}, h2.__s)), d$4(h2.__s, j2.getDerivedStateFromProps(b2, h2.__s))), v2 = h2.props, y2 = h2.state, h2.__v = u2, p2) S2 && null == j2.getDerivedStateFromProps && null != h2.componentWillMount && h2.componentWillMount(), S2 && null != h2.componentDidMount && h2.__h.push(h2.componentDidMount);
      else {
        if (S2 && null == j2.getDerivedStateFromProps && b2 !== v2 && null != h2.componentWillReceiveProps && h2.componentWillReceiveProps(b2, M2), !h2.__e && null != h2.shouldComponentUpdate && false === h2.shouldComponentUpdate(b2, h2.__s, M2) || u2.__v == t2.__v) {
          for (u2.__v != t2.__v && (h2.props = b2, h2.state = h2.__s, h2.__d = false), u2.__e = t2.__e, u2.__k = t2.__k, u2.__k.some(function(n3) {
            n3 && (n3.__ = u2);
          }), $2 = 0; $2 < h2._sb.length; $2++) h2.__h.push(h2._sb[$2]);
          h2._sb = [], h2.__h.length && e2.push(h2);
          break n;
        }
        null != h2.componentWillUpdate && h2.componentWillUpdate(b2, h2.__s, M2), S2 && null != h2.componentDidUpdate && h2.__h.push(function() {
          h2.componentDidUpdate(v2, y2, _2);
        });
      }
      if (h2.context = M2, h2.props = b2, h2.__P = n2, h2.__e = false, P2 = l$3.__r, A2 = 0, S2) {
        for (h2.state = h2.__s, h2.__d = false, P2 && P2(u2), a2 = h2.render(h2.props, h2.state, h2.context), H2 = 0; H2 < h2._sb.length; H2++) h2.__h.push(h2._sb[H2]);
        h2._sb = [];
      } else do {
        h2.__d = false, P2 && P2(u2), a2 = h2.render(h2.props, h2.state, h2.context), h2.state = h2.__s;
      } while (h2.__d && ++A2 < 25);
      h2.state = h2.__s, null != h2.getChildContext && (i2 = d$4(d$4({}, i2), h2.getChildContext())), S2 && !p2 && null != h2.getSnapshotBeforeUpdate && (_2 = h2.getSnapshotBeforeUpdate(v2, y2)), L2 = a2, null != a2 && a2.type === k$2 && null == a2.key && (L2 = N$1(a2.props.children)), f2 = I(n2, w$3(L2) ? L2 : [L2], u2, t2, i2, r2, o2, e2, f2, c2, s2), h2.base = u2.__e, u2.__u &= -161, h2.__h.length && e2.push(h2), m2 && (h2.__E = h2.__ = null);
    } catch (n3) {
      if (u2.__v = null, c2 || null != o2) if (n3.then) {
        for (u2.__u |= c2 ? 160 : 128; f2 && 8 == f2.nodeType && f2.nextSibling; ) f2 = f2.nextSibling;
        o2[o2.indexOf(f2)] = null, u2.__e = f2;
      } else for (T2 = o2.length; T2--; ) g$2(o2[T2]);
      else u2.__e = t2.__e, u2.__k = t2.__k;
      l$3.__e(n3, u2, t2);
    }
    else null == o2 && u2.__v == t2.__v ? (u2.__k = t2.__k, u2.__e = t2.__e) : f2 = u2.__e = V$1(t2.__e, u2, t2, i2, r2, o2, e2, c2, s2);
    return (a2 = l$3.diffed) && a2(u2), 128 & u2.__u ? void 0 : f2;
  }
  function z$1(n2, u2, t2) {
    for (var i2 = 0; i2 < t2.length; i2++) q$2(t2[i2], t2[++i2], t2[++i2]);
    l$3.__c && l$3.__c(u2, n2), n2.some(function(u3) {
      try {
        n2 = u3.__h, u3.__h = [], n2.some(function(n3) {
          n3.call(u3);
        });
      } catch (n3) {
        l$3.__e(n3, u3.__v);
      }
    });
  }
  function N$1(n2) {
    return "object" != typeof n2 || null == n2 || n2.__b && n2.__b > 0 ? n2 : w$3(n2) ? n2.map(N$1) : d$4({}, n2);
  }
  function V$1(u2, t2, i2, r2, o2, e2, f2, c2, s2) {
    var a2, h2, v2, y2, d2, _2, m2, b2 = i2.props, k2 = t2.props, x2 = t2.type;
    if ("svg" == x2 ? o2 = "http://www.w3.org/2000/svg" : "math" == x2 ? o2 = "http://www.w3.org/1998/Math/MathML" : o2 || (o2 = "http://www.w3.org/1999/xhtml"), null != e2) {
      for (a2 = 0; a2 < e2.length; a2++) if ((d2 = e2[a2]) && "setAttribute" in d2 == !!x2 && (x2 ? d2.localName == x2 : 3 == d2.nodeType)) {
        u2 = d2, e2[a2] = null;
        break;
      }
    }
    if (null == u2) {
      if (null == x2) return document.createTextNode(k2);
      u2 = document.createElementNS(o2, x2, k2.is && k2), c2 && (l$3.__m && l$3.__m(t2, e2), c2 = false), e2 = null;
    }
    if (null == x2) b2 === k2 || c2 && u2.data == k2 || (u2.data = k2);
    else {
      if (e2 = e2 && n$2.call(u2.childNodes), b2 = i2.props || p$4, !c2 && null != e2) for (b2 = {}, a2 = 0; a2 < u2.attributes.length; a2++) b2[(d2 = u2.attributes[a2]).name] = d2.value;
      for (a2 in b2) if (d2 = b2[a2], "children" == a2) ;
      else if ("dangerouslySetInnerHTML" == a2) v2 = d2;
      else if (!(a2 in k2)) {
        if ("value" == a2 && "defaultValue" in k2 || "checked" == a2 && "defaultChecked" in k2) continue;
        j$3(u2, a2, null, d2, o2);
      }
      for (a2 in k2) d2 = k2[a2], "children" == a2 ? y2 = d2 : "dangerouslySetInnerHTML" == a2 ? h2 = d2 : "value" == a2 ? _2 = d2 : "checked" == a2 ? m2 = d2 : c2 && "function" != typeof d2 || b2[a2] === d2 || j$3(u2, a2, d2, b2[a2], o2);
      if (h2) c2 || v2 && (h2.__html == v2.__html || h2.__html == u2.innerHTML) || (u2.innerHTML = h2.__html), t2.__k = [];
      else if (v2 && (u2.innerHTML = ""), I("template" == t2.type ? u2.content : u2, w$3(y2) ? y2 : [y2], t2, i2, r2, "foreignObject" == x2 ? "http://www.w3.org/1999/xhtml" : o2, e2, f2, e2 ? e2[0] : i2.__k && S$1(i2, 0), c2, s2), null != e2) for (a2 = e2.length; a2--; ) g$2(e2[a2]);
      c2 || (a2 = "value", "progress" == x2 && null == _2 ? u2.removeAttribute("value") : null != _2 && (_2 !== u2[a2] || "progress" == x2 && !_2 || "option" == x2 && _2 != b2[a2]) && j$3(u2, a2, _2, b2[a2], o2), a2 = "checked", null != m2 && m2 != u2[a2] && j$3(u2, a2, m2, b2[a2], o2));
    }
    return u2;
  }
  function q$2(n2, u2, t2) {
    try {
      if ("function" == typeof n2) {
        var i2 = "function" == typeof n2.__u;
        i2 && n2.__u(), i2 && null == u2 || (n2.__u = n2(u2));
      } else n2.current = u2;
    } catch (n3) {
      l$3.__e(n3, t2);
    }
  }
  function B$2(n2, u2, t2) {
    var i2, r2;
    if (l$3.unmount && l$3.unmount(n2), (i2 = n2.ref) && (i2.current && i2.current != n2.__e || q$2(i2, null, u2)), null != (i2 = n2.__c)) {
      if (i2.componentWillUnmount) try {
        i2.componentWillUnmount();
      } catch (n3) {
        l$3.__e(n3, u2);
      }
      i2.base = i2.__P = null;
    }
    if (i2 = n2.__k) for (r2 = 0; r2 < i2.length; r2++) i2[r2] && B$2(i2[r2], u2, t2 || "function" != typeof n2.type);
    t2 || g$2(n2.__e), n2.__c = n2.__ = n2.__e = void 0;
  }
  function D$2(n2, l2, u2) {
    return this.constructor(n2, u2);
  }
  n$2 = v$4.slice, l$3 = { __e: function(n2, l2, u2, t2) {
    for (var i2, r2, o2; l2 = l2.__; ) if ((i2 = l2.__c) && !i2.__) try {
      if ((r2 = i2.constructor) && null != r2.getDerivedStateFromError && (i2.setState(r2.getDerivedStateFromError(n2)), o2 = i2.__d), null != i2.componentDidCatch && (i2.componentDidCatch(n2, t2 || {}), o2 = i2.__d), o2) return i2.__E = i2;
    } catch (l3) {
      n2 = l3;
    }
    throw n2;
  } }, u$4 = 0, t$2 = function(n2) {
    return null != n2 && null == n2.constructor;
  }, x$1.prototype.setState = function(n2, l2) {
    var u2;
    u2 = null != this.__s && this.__s != this.state ? this.__s : this.__s = d$4({}, this.state), "function" == typeof n2 && (n2 = n2(d$4({}, u2), this.props)), n2 && d$4(u2, n2), null != n2 && this.__v && (l2 && this._sb.push(l2), M$1(this));
  }, x$1.prototype.forceUpdate = function(n2) {
    this.__v && (this.__e = true, n2 && this.__h.push(n2), M$1(this));
  }, x$1.prototype.render = k$2, i$3 = [], o$2 = "function" == typeof Promise ? Promise.prototype.then.bind(Promise.resolve()) : setTimeout, e$2 = function(n2, l2) {
    return n2.__v.__b - l2.__v.__b;
  }, $.__r = 0, f$4 = /(PointerCapture)$|Capture$/i, c$4 = 0, s$3 = F$2(false), a$3 = F$2(true);
  var n$1 = /[\s\n\\/='"\0<>]/, o$1 = /^(xlink|xmlns|xml)([A-Z])/, i$2 = /^(?:accessK|auto[A-Z]|cell|ch|col|cont|cross|dateT|encT|form[A-Z]|frame|hrefL|inputM|maxL|minL|noV|playsI|popoverT|readO|rowS|src[A-Z]|tabI|useM|item[A-Z])/, a$2 = /^ac|^ali|arabic|basel|cap|clipPath$|clipRule$|color|dominant|enable|fill|flood|font|glyph[^R]|horiz|image|letter|lighting|marker[^WUH]|overline|panose|pointe|paint|rendering|shape|stop|strikethrough|stroke|text[^L]|transform|underline|unicode|units|^v[^i]|^w|^xH/, c$3 = /* @__PURE__ */ new Set(["draggable", "spellcheck"]), s$2 = /["&<]/;
  function l$2(e2) {
    if (0 === e2.length || false === s$2.test(e2)) return e2;
    for (var t2 = 0, r2 = 0, n2 = "", o2 = ""; r2 < e2.length; r2++) {
      switch (e2.charCodeAt(r2)) {
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
      r2 !== t2 && (n2 += e2.slice(t2, r2)), n2 += o2, t2 = r2 + 1;
    }
    return r2 !== t2 && (n2 += e2.slice(t2, r2)), n2;
  }
  var u$3 = {}, f$3 = /* @__PURE__ */ new Set(["animation-iteration-count", "border-image-outset", "border-image-slice", "border-image-width", "box-flex", "box-flex-group", "box-ordinal-group", "column-count", "fill-opacity", "flex", "flex-grow", "flex-negative", "flex-order", "flex-positive", "flex-shrink", "flood-opacity", "font-weight", "grid-column", "grid-row", "line-clamp", "line-height", "opacity", "order", "orphans", "stop-opacity", "stroke-dasharray", "stroke-dashoffset", "stroke-miterlimit", "stroke-opacity", "stroke-width", "tab-size", "widows", "z-index", "zoom"]), p$3 = /[A-Z]/g;
  function h$3(e2) {
    var t2 = "";
    for (var r2 in e2) {
      var n2 = e2[r2];
      if (null != n2 && "" !== n2) {
        var o2 = "-" == r2[0] ? r2 : u$3[r2] || (u$3[r2] = r2.replace(p$3, "-$&").toLowerCase()), i2 = ";";
        "number" != typeof n2 || o2.startsWith("--") || f$3.has(o2) || (i2 = "px;"), t2 = t2 + o2 + ":" + n2 + i2;
      }
    }
    return t2 || void 0;
  }
  function d$3() {
    this.__d = true;
  }
  function v$3(e2, t2) {
    return { __v: e2, context: t2, props: e2.props, setState: d$3, forceUpdate: d$3, __d: true, __h: new Array(0) };
  }
  var k$1, w$2, x, C$1, S = {}, L = [], E$2 = Array.isArray, T$2 = Object.assign, j$2 = "";
  function D$1(n2, o2, i2) {
    var a2 = l$3.__s;
    l$3.__s = true, k$1 = l$3.__b, w$2 = l$3.diffed, x = l$3.__r, C$1 = l$3.unmount;
    var c2 = _$1(k$2, null);
    c2.__k = [n2];
    try {
      var s2 = U$1(n2, o2 || S, false, void 0, c2, false, i2);
      return E$2(s2) ? s2.join(j$2) : s2;
    } catch (e2) {
      if (e2.then) throw new Error('Use "renderToStringAsync" for suspenseful rendering.');
      throw e2;
    } finally {
      l$3.__c && l$3.__c(n2, L), l$3.__s = a2, L.length = 0;
    }
  }
  function P$1(e2, t2) {
    var r2, n2 = e2.type, o2 = true;
    return e2.__c ? (o2 = false, (r2 = e2.__c).state = r2.__s) : r2 = new n2(e2.props, t2), e2.__c = r2, r2.__v = e2, r2.props = e2.props, r2.context = t2, r2.__d = true, null == r2.state && (r2.state = S), null == r2.__s && (r2.__s = r2.state), n2.getDerivedStateFromProps ? r2.state = T$2({}, r2.state, n2.getDerivedStateFromProps(r2.props, r2.state)) : o2 && r2.componentWillMount ? (r2.componentWillMount(), r2.state = r2.__s !== r2.state ? r2.__s : r2.state) : !o2 && r2.componentWillUpdate && r2.componentWillUpdate(), x && x(e2), r2.render(r2.props, r2.state, t2);
  }
  function U$1(t2, s2, u2, f2, p2, d2, _2) {
    if (null == t2 || true === t2 || false === t2 || t2 === j$2) return j$2;
    var m2 = typeof t2;
    if ("object" != m2) return "function" == m2 ? j$2 : "string" == m2 ? l$2(t2) : t2 + j$2;
    if (E$2(t2)) {
      var y2, g2 = j$2;
      p2.__k = t2;
      for (var b2 = t2.length, A2 = 0; A2 < b2; A2++) {
        var L2 = t2[A2];
        if (null != L2 && "boolean" != typeof L2) {
          var D2, F2 = U$1(L2, s2, u2, f2, p2, d2, _2);
          "string" == typeof F2 ? g2 += F2 : (y2 || (y2 = new Array(b2)), g2 && y2.push(g2), g2 = j$2, E$2(F2) ? (D2 = y2).push.apply(D2, F2) : y2.push(F2));
        }
      }
      return y2 ? (g2 && y2.push(g2), y2) : g2;
    }
    if (void 0 !== t2.constructor) return j$2;
    t2.__ = p2, k$1 && k$1(t2);
    var M2 = t2.type, W2 = t2.props;
    if ("function" == typeof M2) {
      var $2, z2, H2, N2 = s2;
      if (M2 === k$2) {
        if ("tpl" in W2) {
          for (var q2 = j$2, B2 = 0; B2 < W2.tpl.length; B2++) if (q2 += W2.tpl[B2], W2.exprs && B2 < W2.exprs.length) {
            var I2 = W2.exprs[B2];
            if (null == I2) continue;
            "object" != typeof I2 || void 0 !== I2.constructor && !E$2(I2) ? q2 += I2 : q2 += U$1(I2, s2, u2, f2, t2, d2, _2);
          }
          return q2;
        }
        if ("UNSTABLE_comment" in W2) return "<!--" + l$2(W2.UNSTABLE_comment) + "-->";
        z2 = W2.children;
      } else {
        if (null != ($2 = M2.contextType)) {
          var O2 = s2[$2.__c];
          N2 = O2 ? O2.props.value : $2.__;
        }
        var R = M2.prototype && "function" == typeof M2.prototype.render;
        if (R) z2 = P$1(t2, N2), H2 = t2.__c;
        else {
          t2.__c = H2 = v$3(t2, N2);
          for (var V2 = 0; H2.__d && V2++ < 25; ) H2.__d = false, x && x(t2), z2 = M2.call(H2, W2, N2);
          H2.__d = true;
        }
        if (null != H2.getChildContext && (s2 = T$2({}, s2, H2.getChildContext())), R && l$3.errorBoundaries && (M2.getDerivedStateFromError || H2.componentDidCatch)) {
          z2 = null != z2 && z2.type === k$2 && null == z2.key && null == z2.props.tpl ? z2.props.children : z2;
          try {
            return U$1(z2, s2, u2, f2, t2, d2, _2);
          } catch (e2) {
            return M2.getDerivedStateFromError && (H2.__s = M2.getDerivedStateFromError(e2)), H2.componentDidCatch && H2.componentDidCatch(e2, S), H2.__d ? (z2 = P$1(t2, s2), null != (H2 = t2.__c).getChildContext && (s2 = T$2({}, s2, H2.getChildContext())), U$1(z2 = null != z2 && z2.type === k$2 && null == z2.key && null == z2.props.tpl ? z2.props.children : z2, s2, u2, f2, t2, d2, _2)) : j$2;
          } finally {
            w$2 && w$2(t2), C$1 && C$1(t2);
          }
        }
      }
      z2 = null != z2 && z2.type === k$2 && null == z2.key && null == z2.props.tpl ? z2.props.children : z2;
      try {
        var K2 = U$1(z2, s2, u2, f2, t2, d2, _2);
        return w$2 && w$2(t2), l$3.unmount && l$3.unmount(t2), K2;
      } catch (r2) {
        if (_2 && _2.onError) {
          var G2 = _2.onError(r2, t2, function(e2, t3) {
            return U$1(e2, s2, u2, f2, t3, d2, _2);
          });
          if (void 0 !== G2) return G2;
          var J2 = l$3.__e;
          return J2 && J2(r2, t2), j$2;
        }
        throw r2;
      }
    }
    var Q2, X2 = "<" + M2, Y = j$2;
    for (var ee in W2) {
      var te = W2[ee];
      if ("function" != typeof te || "class" === ee || "className" === ee) {
        switch (ee) {
          case "children":
            Q2 = te;
            continue;
          case "key":
          case "ref":
          case "__self":
          case "__source":
            continue;
          case "htmlFor":
            if ("for" in W2) continue;
            ee = "for";
            break;
          case "className":
            if ("class" in W2) continue;
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
            switch (ee = "value", M2) {
              case "textarea":
                Q2 = te;
                continue;
              case "select":
                f2 = te;
                continue;
              case "option":
                f2 != te || "selected" in W2 || (X2 += " selected");
            }
            break;
          case "dangerouslySetInnerHTML":
            Y = te && te.__html;
            continue;
          case "style":
            "object" == typeof te && (te = h$3(te));
            break;
          case "acceptCharset":
            ee = "accept-charset";
            break;
          case "httpEquiv":
            ee = "http-equiv";
            break;
          default:
            if (o$1.test(ee)) ee = ee.replace(o$1, "$1:$2").toLowerCase();
            else {
              if (n$1.test(ee)) continue;
              "-" !== ee[4] && !c$3.has(ee) || null == te ? u2 ? a$2.test(ee) && (ee = "panose1" === ee ? "panose-1" : ee.replace(/([A-Z])/g, "-$1").toLowerCase()) : i$2.test(ee) && (ee = ee.toLowerCase()) : te += j$2;
            }
        }
        null != te && false !== te && (X2 = true === te || te === j$2 ? X2 + " " + ee : X2 + " " + ee + '="' + ("string" == typeof te ? l$2(te) : te + j$2) + '"');
      }
    }
    if (n$1.test(M2)) throw new Error(M2 + " is not a valid HTML tag name in " + X2 + ">");
    if (Y || ("string" == typeof Q2 ? Y = l$2(Q2) : null != Q2 && false !== Q2 && true !== Q2 && (Y = U$1(Q2, s2, "svg" === M2 || "foreignObject" !== M2 && u2, f2, t2, d2, _2))), w$2 && w$2(t2), C$1 && C$1(t2), !Y && Z.has(M2)) return X2 + "/>";
    var re = "</" + M2 + ">", ne = X2 + ">";
    return E$2(Y) ? [ne].concat(Y, [re]) : "string" != typeof Y ? [ne, Y, re] : ne + Y + re;
  }
  var Z = /* @__PURE__ */ new Set(["area", "base", "br", "col", "command", "embed", "hr", "img", "input", "keygen", "link", "meta", "param", "source", "track", "wbr"]), F$1 = D$1;
  var t$1, r$1, u$2, i$1, o = 0, f$2 = [], c$2 = l$3, e$1 = c$2.__b, a$1 = c$2.__r, v$2 = c$2.diffed, l$1 = c$2.__c, m = c$2.unmount, s$1 = c$2.__;
  function p$2(n2, t2) {
    c$2.__h && c$2.__h(r$1, n2, o || t2), o = 0;
    var u2 = r$1.__H || (r$1.__H = { __: [], __h: [] });
    return n2 >= u2.__.length && u2.__.push({}), u2.__[n2];
  }
  function d$2(n2) {
    return o = 1, h$2(D, n2);
  }
  function h$2(n2, u2, i2) {
    var o2 = p$2(t$1++, 2);
    if (o2.t = n2, !o2.__c && (o2.__ = [i2 ? i2(u2) : D(void 0, u2), function(n3) {
      var t2 = o2.__N ? o2.__N[0] : o2.__[0], r2 = o2.t(t2, n3);
      t2 !== r2 && (o2.__N = [r2, o2.__[1]], o2.__c.setState({}));
    }], o2.__c = r$1, !r$1.__f)) {
      var f2 = function(n3, t2, r2) {
        if (!o2.__c.__H) return true;
        var u3 = o2.__c.__H.__.filter(function(n4) {
          return !!n4.__c;
        });
        if (u3.every(function(n4) {
          return !n4.__N;
        })) return !c2 || c2.call(this, n3, t2, r2);
        var i3 = o2.__c.props !== n3;
        return u3.forEach(function(n4) {
          if (n4.__N) {
            var t3 = n4.__[0];
            n4.__ = n4.__N, n4.__N = void 0, t3 !== n4.__[0] && (i3 = true);
          }
        }), c2 && c2.call(this, n3, t2, r2) || i3;
      };
      r$1.__f = true;
      var c2 = r$1.shouldComponentUpdate, e2 = r$1.componentWillUpdate;
      r$1.componentWillUpdate = function(n3, t2, r2) {
        if (this.__e) {
          var u3 = c2;
          c2 = void 0, f2(n3, t2, r2), c2 = u3;
        }
        e2 && e2.call(this, n3, t2, r2);
      }, r$1.shouldComponentUpdate = f2;
    }
    return o2.__N || o2.__;
  }
  function y$1(n2, u2) {
    var i2 = p$2(t$1++, 3);
    !c$2.__s && C(i2.__H, u2) && (i2.__ = n2, i2.u = u2, r$1.__H.__h.push(i2));
  }
  function A(n2) {
    return o = 5, T$1(function() {
      return { current: n2 };
    }, []);
  }
  function T$1(n2, r2) {
    var u2 = p$2(t$1++, 7);
    return C(u2.__H, r2) && (u2.__ = n2(), u2.__H = r2, u2.__h = n2), u2.__;
  }
  function q$1(n2, t2) {
    return o = 8, T$1(function() {
      return n2;
    }, t2);
  }
  function j$1() {
    for (var n2; n2 = f$2.shift(); ) if (n2.__P && n2.__H) try {
      n2.__H.__h.forEach(z), n2.__H.__h.forEach(B$1), n2.__H.__h = [];
    } catch (t2) {
      n2.__H.__h = [], c$2.__e(t2, n2.__v);
    }
  }
  c$2.__b = function(n2) {
    r$1 = null, e$1 && e$1(n2);
  }, c$2.__ = function(n2, t2) {
    n2 && t2.__k && t2.__k.__m && (n2.__m = t2.__k.__m), s$1 && s$1(n2, t2);
  }, c$2.__r = function(n2) {
    a$1 && a$1(n2), t$1 = 0;
    var i2 = (r$1 = n2.__c).__H;
    i2 && (u$2 === r$1 ? (i2.__h = [], r$1.__h = [], i2.__.forEach(function(n3) {
      n3.__N && (n3.__ = n3.__N), n3.u = n3.__N = void 0;
    })) : (i2.__h.forEach(z), i2.__h.forEach(B$1), i2.__h = [], t$1 = 0)), u$2 = r$1;
  }, c$2.diffed = function(n2) {
    v$2 && v$2(n2);
    var t2 = n2.__c;
    t2 && t2.__H && (t2.__H.__h.length && (1 !== f$2.push(t2) && i$1 === c$2.requestAnimationFrame || ((i$1 = c$2.requestAnimationFrame) || w$1)(j$1)), t2.__H.__.forEach(function(n3) {
      n3.u && (n3.__H = n3.u), n3.u = void 0;
    })), u$2 = r$1 = null;
  }, c$2.__c = function(n2, t2) {
    t2.some(function(n3) {
      try {
        n3.__h.forEach(z), n3.__h = n3.__h.filter(function(n4) {
          return !n4.__ || B$1(n4);
        });
      } catch (r2) {
        t2.some(function(n4) {
          n4.__h && (n4.__h = []);
        }), t2 = [], c$2.__e(r2, n3.__v);
      }
    }), l$1 && l$1(n2, t2);
  }, c$2.unmount = function(n2) {
    m && m(n2);
    var t2, r2 = n2.__c;
    r2 && r2.__H && (r2.__H.__.forEach(function(n3) {
      try {
        z(n3);
      } catch (n4) {
        t2 = n4;
      }
    }), r2.__H = void 0, t2 && c$2.__e(t2, r2.__v));
  };
  var k = "function" == typeof requestAnimationFrame;
  function w$1(n2) {
    var t2, r2 = function() {
      clearTimeout(u2), k && cancelAnimationFrame(t2), setTimeout(n2);
    }, u2 = setTimeout(r2, 35);
    k && (t2 = requestAnimationFrame(r2));
  }
  function z(n2) {
    var t2 = r$1, u2 = n2.__c;
    "function" == typeof u2 && (n2.__c = void 0, u2()), r$1 = t2;
  }
  function B$1(n2) {
    var t2 = r$1;
    n2.__c = n2.__(), r$1 = t2;
  }
  function C(n2, t2) {
    return !n2 || n2.length !== t2.length || t2.some(function(t3, r2) {
      return t3 !== n2[r2];
    });
  }
  function D(n2, t2) {
    return "function" == typeof t2 ? t2(n2) : t2;
  }
  const i = Symbol.for("preact-signals");
  function t() {
    if (r > 1) {
      r--;
      return;
    }
    let i2, t2 = false;
    while (void 0 !== s) {
      let o2 = s;
      s = void 0;
      f$1++;
      while (void 0 !== o2) {
        const n2 = o2.o;
        o2.o = void 0;
        o2.f &= -3;
        if (!(8 & o2.f) && v$1(o2)) try {
          o2.c();
        } catch (o3) {
          if (!t2) {
            i2 = o3;
            t2 = true;
          }
        }
        o2 = n2;
      }
    }
    f$1 = 0;
    r--;
    if (t2) throw i2;
  }
  let n, s;
  function h$1(i2) {
    const t2 = n;
    n = void 0;
    try {
      return i2();
    } finally {
      n = t2;
    }
  }
  let r = 0, f$1 = 0, e = 0;
  function c$1(i2) {
    if (void 0 === n) return;
    let t2 = i2.n;
    if (void 0 === t2 || t2.t !== n) {
      t2 = { i: 0, S: i2, p: n.s, n: void 0, t: n, e: void 0, x: void 0, r: t2 };
      if (void 0 !== n.s) n.s.n = t2;
      n.s = t2;
      i2.n = t2;
      if (32 & n.f) i2.S(t2);
      return t2;
    } else if (-1 === t2.i) {
      t2.i = 0;
      if (void 0 !== t2.n) {
        t2.n.p = t2.p;
        if (void 0 !== t2.p) t2.p.n = t2.n;
        t2.p = n.s;
        t2.n = void 0;
        n.s.n = t2;
        n.s = t2;
      }
      return t2;
    }
  }
  function u$1(i2, t2) {
    this.v = i2;
    this.i = 0;
    this.n = void 0;
    this.t = void 0;
    this.W = null == t2 ? void 0 : t2.watched;
    this.Z = null == t2 ? void 0 : t2.unwatched;
  }
  u$1.prototype.brand = i;
  u$1.prototype.h = function() {
    return true;
  };
  u$1.prototype.S = function(i2) {
    const t2 = this.t;
    if (t2 !== i2 && void 0 === i2.e) {
      i2.x = t2;
      this.t = i2;
      if (void 0 !== t2) t2.e = i2;
      else h$1(() => {
        var i3;
        null == (i3 = this.W) || i3.call(this);
      });
    }
  };
  u$1.prototype.U = function(i2) {
    if (void 0 !== this.t) {
      const t2 = i2.e, o2 = i2.x;
      if (void 0 !== t2) {
        t2.x = o2;
        i2.e = void 0;
      }
      if (void 0 !== o2) {
        o2.e = t2;
        i2.x = void 0;
      }
      if (i2 === this.t) {
        this.t = o2;
        if (void 0 === o2) h$1(() => {
          var i3;
          null == (i3 = this.Z) || i3.call(this);
        });
      }
    }
  };
  u$1.prototype.subscribe = function(i2) {
    return E$1(() => {
      const t2 = this.value, o2 = n;
      n = void 0;
      try {
        i2(t2);
      } finally {
        n = o2;
      }
    });
  };
  u$1.prototype.valueOf = function() {
    return this.value;
  };
  u$1.prototype.toString = function() {
    return this.value + "";
  };
  u$1.prototype.toJSON = function() {
    return this.value;
  };
  u$1.prototype.peek = function() {
    const i2 = n;
    n = void 0;
    try {
      return this.value;
    } finally {
      n = i2;
    }
  };
  Object.defineProperty(u$1.prototype, "value", { get() {
    const i2 = c$1(this);
    if (void 0 !== i2) i2.i = this.i;
    return this.v;
  }, set(i2) {
    if (i2 !== this.v) {
      if (f$1 > 100) throw new Error("Cycle detected");
      this.v = i2;
      this.i++;
      e++;
      r++;
      try {
        for (let i3 = this.t; void 0 !== i3; i3 = i3.x) i3.t.N();
      } finally {
        t();
      }
    }
  } });
  function d$1(i2, t2) {
    return new u$1(i2, t2);
  }
  function v$1(i2) {
    for (let t2 = i2.s; void 0 !== t2; t2 = t2.n) if (t2.S.i !== t2.i || !t2.S.h() || t2.S.i !== t2.i) return true;
    return false;
  }
  function l(i2) {
    for (let t2 = i2.s; void 0 !== t2; t2 = t2.n) {
      const o2 = t2.S.n;
      if (void 0 !== o2) t2.r = o2;
      t2.S.n = t2;
      t2.i = -1;
      if (void 0 === t2.n) {
        i2.s = t2;
        break;
      }
    }
  }
  function y(i2) {
    let t2, o2 = i2.s;
    while (void 0 !== o2) {
      const i3 = o2.p;
      if (-1 === o2.i) {
        o2.S.U(o2);
        if (void 0 !== i3) i3.n = o2.n;
        if (void 0 !== o2.n) o2.n.p = i3;
      } else t2 = o2;
      o2.S.n = o2.r;
      if (void 0 !== o2.r) o2.r = void 0;
      o2 = i3;
    }
    i2.s = t2;
  }
  function a(i2, t2) {
    u$1.call(this, void 0);
    this.x = i2;
    this.s = void 0;
    this.g = e - 1;
    this.f = 4;
    this.W = null == t2 ? void 0 : t2.watched;
    this.Z = null == t2 ? void 0 : t2.unwatched;
  }
  a.prototype = new u$1();
  a.prototype.h = function() {
    this.f &= -3;
    if (1 & this.f) return false;
    if (32 == (36 & this.f)) return true;
    this.f &= -5;
    if (this.g === e) return true;
    this.g = e;
    this.f |= 1;
    if (this.i > 0 && !v$1(this)) {
      this.f &= -2;
      return true;
    }
    const i2 = n;
    try {
      l(this);
      n = this;
      const i3 = this.x();
      if (16 & this.f || this.v !== i3 || 0 === this.i) {
        this.v = i3;
        this.f &= -17;
        this.i++;
      }
    } catch (i3) {
      this.v = i3;
      this.f |= 16;
      this.i++;
    }
    n = i2;
    y(this);
    this.f &= -2;
    return true;
  };
  a.prototype.S = function(i2) {
    if (void 0 === this.t) {
      this.f |= 36;
      for (let i3 = this.s; void 0 !== i3; i3 = i3.n) i3.S.S(i3);
    }
    u$1.prototype.S.call(this, i2);
  };
  a.prototype.U = function(i2) {
    if (void 0 !== this.t) {
      u$1.prototype.U.call(this, i2);
      if (void 0 === this.t) {
        this.f &= -33;
        for (let i3 = this.s; void 0 !== i3; i3 = i3.n) i3.S.U(i3);
      }
    }
  };
  a.prototype.N = function() {
    if (!(2 & this.f)) {
      this.f |= 6;
      for (let i2 = this.t; void 0 !== i2; i2 = i2.x) i2.t.N();
    }
  };
  Object.defineProperty(a.prototype, "value", { get() {
    if (1 & this.f) throw new Error("Cycle detected");
    const i2 = c$1(this);
    this.h();
    if (void 0 !== i2) i2.i = this.i;
    if (16 & this.f) throw this.v;
    return this.v;
  } });
  function w(i2, t2) {
    return new a(i2, t2);
  }
  function _(i2) {
    const o2 = i2.u;
    i2.u = void 0;
    if ("function" == typeof o2) {
      r++;
      const s2 = n;
      n = void 0;
      try {
        o2();
      } catch (t2) {
        i2.f &= -2;
        i2.f |= 8;
        g$1(i2);
        throw t2;
      } finally {
        n = s2;
        t();
      }
    }
  }
  function g$1(i2) {
    for (let t2 = i2.s; void 0 !== t2; t2 = t2.n) t2.S.U(t2);
    i2.x = void 0;
    i2.s = void 0;
    _(i2);
  }
  function p$1(i2) {
    if (n !== this) throw new Error("Out-of-order effect");
    y(this);
    n = i2;
    this.f &= -2;
    if (8 & this.f) g$1(this);
    t();
  }
  function b(i2) {
    this.x = i2;
    this.u = void 0;
    this.s = void 0;
    this.o = void 0;
    this.f = 32;
  }
  b.prototype.c = function() {
    const i2 = this.S();
    try {
      if (8 & this.f) return;
      if (void 0 === this.x) return;
      const t2 = this.x();
      if ("function" == typeof t2) this.u = t2;
    } finally {
      i2();
    }
  };
  b.prototype.S = function() {
    if (1 & this.f) throw new Error("Cycle detected");
    this.f |= 1;
    this.f &= -9;
    _(this);
    l(this);
    r++;
    const i2 = n;
    n = this;
    return p$1.bind(this, i2);
  };
  b.prototype.N = function() {
    if (!(2 & this.f)) {
      this.f |= 2;
      this.o = s;
      s = this;
    }
  };
  b.prototype.d = function() {
    this.f |= 8;
    if (!(1 & this.f)) g$1(this);
  };
  function E$1(i2) {
    const t2 = new b(i2);
    try {
      t2.c();
    } catch (i3) {
      t2.d();
      throw i3;
    }
    return t2.d.bind(t2);
  }
  function c(t2, e2) {
    l$3[t2] = e2.bind(null, l$3[t2] || (() => {
    }));
  }
  let h;
  function d(t2) {
    if (h) h();
    h = t2 && t2.S();
  }
  function p({ data: t2 }) {
    const i2 = useSignal(t2);
    i2.value = t2;
    const o2 = T$1(() => {
      let t3 = this.__v;
      while (t3 = t3.__) if (t3.__c) {
        t3.__c.__$f |= 4;
        break;
      }
      this.__$u.c = () => {
        var t4;
        const i3 = this.__$u.S(), n2 = o2.value;
        i3();
        if (t$2(n2) || 3 !== (null == (t4 = this.base) ? void 0 : t4.nodeType)) {
          this.__$f |= 1;
          this.setState({});
        } else this.base.data = n2;
      };
      return w(() => {
        let t4 = i2.value.value;
        return 0 === t4 ? 0 : true === t4 ? "" : t4 || "";
      });
    }, []);
    return o2.value;
  }
  p.displayName = "_st";
  Object.defineProperties(u$1.prototype, { constructor: { configurable: true, value: void 0 }, type: { configurable: true, value: p }, props: { configurable: true, get() {
    return { data: this };
  } }, __b: { configurable: true, value: 1 } });
  c("__b", (t2, i2) => {
    if ("string" == typeof i2.type) {
      let t3, e2 = i2.props;
      for (let n2 in e2) {
        if ("children" === n2) continue;
        let o2 = e2[n2];
        if (o2 instanceof u$1) {
          if (!t3) i2.__np = t3 = {};
          t3[n2] = o2;
          e2[n2] = o2.peek();
        }
      }
    }
    t2(i2);
  });
  c("__r", (t2, i2) => {
    d();
    let e2, n2 = i2.__c;
    if (n2) {
      n2.__$f &= -2;
      e2 = n2.__$u;
      if (void 0 === e2) n2.__$u = e2 = function(t3) {
        let i3;
        E$1(function() {
          i3 = this;
        });
        i3.c = () => {
          n2.__$f |= 1;
          n2.setState({});
        };
        return i3;
      }();
    }
    d(e2);
    t2(i2);
  });
  c("__e", (t2, i2, e2, n2) => {
    d();
    t2(i2, e2, n2);
  });
  c("diffed", (t2, i2) => {
    d();
    let e2;
    if ("string" == typeof i2.type && (e2 = i2.__e)) {
      let t3 = i2.__np, n2 = i2.props;
      if (t3) {
        let i3 = e2.U;
        if (i3) for (let e3 in i3) {
          let n3 = i3[e3];
          if (void 0 !== n3 && !(e3 in t3)) {
            n3.d();
            i3[e3] = void 0;
          }
        }
        else {
          i3 = {};
          e2.U = i3;
        }
        for (let o2 in t3) {
          let r2 = i3[o2], f2 = t3[o2];
          if (void 0 === r2) {
            r2 = v(e2, o2, f2, n2);
            i3[o2] = r2;
          } else r2.o(f2, n2);
        }
      }
    }
    t2(i2);
  });
  function v(t2, i2, e2, n2) {
    const o2 = i2 in t2 && void 0 === t2.ownerSVGElement, r2 = d$1(e2);
    return { o: (t3, i3) => {
      r2.value = t3;
      n2 = i3;
    }, d: E$1(() => {
      const e3 = r2.value.value;
      if (n2[i2] !== e3) {
        n2[i2] = e3;
        if (o2) t2[i2] = e3;
        else if (e3) t2.setAttribute(i2, e3);
        else t2.removeAttribute(i2);
      }
    }) };
  }
  c("unmount", (t2, i2) => {
    if ("string" == typeof i2.type) {
      let t3 = i2.__e;
      if (t3) {
        const i3 = t3.U;
        if (i3) {
          t3.U = void 0;
          for (let t4 in i3) {
            let e2 = i3[t4];
            if (e2) e2.d();
          }
        }
      }
    } else {
      let t3 = i2.__c;
      if (t3) {
        const i3 = t3.__$u;
        if (i3) {
          t3.__$u = void 0;
          i3.d();
        }
      }
    }
    t2(i2);
  });
  c("__h", (t2, i2, e2, n2) => {
    if (n2 < 3 || 9 === n2) i2.__$f |= 2;
    t2(i2, e2, n2);
  });
  x$1.prototype.shouldComponentUpdate = function(t2, i2) {
    const e2 = this.__$u, n2 = e2 && void 0 !== e2.s;
    for (let t3 in i2) return true;
    if (this.__f || "boolean" == typeof this.u && true === this.u) {
      const t3 = 2 & this.__$f;
      if (!(n2 || t3 || 4 & this.__$f)) return true;
      if (1 & this.__$f) return true;
    } else {
      if (!(n2 || 4 & this.__$f)) return true;
      if (3 & this.__$f) return true;
    }
    for (let i3 in t2) if ("__source" !== i3 && t2[i3] !== this.props[i3]) return true;
    for (let i3 in this.props) if (!(i3 in t2)) return true;
    return false;
  };
  function useSignal(t2) {
    return T$1(() => d$1(t2), []);
  }
  const activeOrders = d$1(/* @__PURE__ */ new Map());
  d$1(null);
  const products = d$1([]);
  const cartItems = d$1([]);
  const loading = d$1(false);
  const searchQuery = d$1("");
  const currentLanguage = d$1("ru");
  d$1([]);
  const productsLoading = d$1(false);
  const productsError = d$1(null);
  const filteredProducts = w(() => {
    const query = searchQuery.value.toLowerCase();
    const productList = products.value;
    if (!productList || !Array.isArray(productList)) return [];
    if (!query) return productList;
    return productList.filter(
      (p2) => p2.name && p2.name[currentLanguage.value] && p2.name[currentLanguage.value].toLowerCase().includes(query)
    );
  });
  const cartTotal = w(() => {
    const items = cartItems.value;
    if (!items || !Array.isArray(items)) return 0;
    return items.reduce(
      (sum, item) => sum + item.price * item.quantity,
      0
    );
  });
  const cartCount = w(() => {
    const items = cartItems.value;
    return items && Array.isArray(items) ? items.length : 0;
  });
  w(() => {
    const items = cartItems.value;
    return items && Array.isArray(items) ? items.length : 0;
  });
  const hasActiveOrders = w(() => activeOrders.value.size > 0);
  const orderStatusText = w(() => {
    const orderMap = activeOrders.value;
    if (orderMap.size === 0) return "";
    const statuses = Array.from(orderMap.values()).map((order) => {
      switch (order.status) {
        case "pending":
          return "Ожидает подтверждения";
        case "confirmed":
          return "Подтвержден";
        case "preparing":
          return "Готовится";
        case "ready":
          return "Готов к выдаче";
        case "delivering":
          return "В пути";
        case "delivered":
          return "Доставлен";
        default:
          return "Неизвестный статус";
      }
    });
    return statuses.join(", ");
  });
  const API_URL = typeof window !== "undefined" ? "/api" : "https://enddel.com/api";
  async function loadProducts() {
    if (products.value.length > 0) return;
    loading.value = true;
    productsLoading.value = true;
    try {
      const res = await fetch(`${API_URL}/products`);
      const data = await res.json();
      const loadedProducts = data.products || data || [];
      products.value = loadedProducts.map((p2) => ({
        ...p2,
        titles: p2.titles || p2.name
      }));
    } catch (error) {
      console.error("Failed to load products:", error);
      productsError.value = "Ошибка загрузки товаров";
      products.value = [];
    } finally {
      loading.value = false;
      productsLoading.value = false;
    }
  }
  function addToCart(product, quantity = 1) {
    const existing = cartItems.value.find((item) => item.id === product.id);
    if (existing) {
      cartItems.value = cartItems.value.map(
        (item) => item.id === product.id ? { ...item, quantity: item.quantity + quantity } : item
      );
    } else {
      const titles = product.titles || product.name;
      cartItems.value = [...cartItems.value, {
        id: product.id,
        title: titles[currentLanguage.value],
        titles,
        price: product.price,
        quantity,
        unit: product.unit,
        step: product.step,
        discounts: product.discounts
      }];
    }
    saveCartToStorage();
  }
  function addItemToCart(item) {
    const existing = cartItems.value.find((cartItem) => cartItem.id === item.id);
    if (existing) {
      cartItems.value = cartItems.value.map(
        (cartItem) => cartItem.id === item.id ? { ...cartItem, quantity: cartItem.quantity + item.quantity } : cartItem
      );
    } else {
      cartItems.value = [...cartItems.value, item];
    }
    saveCartToStorage();
  }
  function removeFromCart(productId) {
    cartItems.value = cartItems.value.filter((item) => item.id !== productId);
    saveCartToStorage();
  }
  function removeItemFromCart(productId) {
    removeFromCart(productId);
  }
  function updateCartItem(item) {
    cartItems.value = cartItems.value.map(
      (cartItem) => cartItem.id === item.id ? item : cartItem
    );
    saveCartToStorage();
  }
  function getCartItemById(id, fromRecipe) {
    const items = cartItems.value;
    if (!items || !Array.isArray(items)) return void 0;
    return items.find((item) => item.id === id);
  }
  function getProductStock(id) {
    const product = products.value.find((p2) => p2.id === id);
    return (product == null ? void 0 : product.stock_quantity) ?? 0;
  }
  function saveCartToStorage() {
    if (typeof window === "undefined") return;
    localStorage.setItem("cart", JSON.stringify(cartItems.value));
  }
  function getImageUrl(path) {
    if (!path) return "";
    if (path.startsWith("http")) return path;
    if (path.startsWith("data:")) return path;
    if (path.startsWith("blob:")) return path;
    const cleanPath = path.startsWith("/") ? path.substring(1) : path;
    return `https://enddel.com/images/${cleanPath}`;
  }
  function getLocalizedTitle(titles, lang = currentLanguage.value) {
    if (!titles) return "Без названия";
    return titles[lang] || titles.ru || "Без названия";
  }
  var f = 0;
  function u(e2, t2, n2, o2, i2, u2) {
    t2 || (t2 = {});
    var a2, c2, p2 = t2;
    if ("ref" in p2) for (c2 in p2 = {}, t2) "ref" == c2 ? a2 = t2[c2] : p2[c2] = t2[c2];
    var l2 = { type: e2, props: p2, key: n2, ref: a2, __k: null, __: null, __b: 0, __e: null, __c: null, constructor: void 0, __v: --f, __i: -1, __u: 0, __source: i2, __self: u2 };
    if ("function" == typeof e2 && (a2 = e2.defaultProps)) for (c2 in a2) void 0 === p2[c2] && (p2[c2] = a2[c2]);
    return l$3.vnode && l$3.vnode(l2), l2;
  }
  const SimpleBottomSheet = ({ isOpen, onClose, children }) => {
    const [shouldRender, setShouldRender] = d$2(false);
    const [isAnimating, setIsAnimating] = d$2(false);
    const sheetRef = A(null);
    y$1(() => {
      if (isOpen && !shouldRender) {
        setShouldRender(true);
        setTimeout(() => {
          setIsAnimating(true);
        }, 10);
      } else if (!isOpen && shouldRender && isAnimating) {
        setIsAnimating(false);
      }
    }, [isOpen, shouldRender, isAnimating]);
    y$1(() => {
      if (!isOpen && shouldRender) {
        const removeTimeout = setTimeout(() => {
          setShouldRender(false);
        }, 850);
        return () => clearTimeout(removeTimeout);
      }
    }, [isOpen, shouldRender]);
    y$1(() => {
      if (!isOpen) return;
      const handleKeyDown = (event) => {
        if (event.key === "Escape") {
          onClose();
        }
      };
      document.addEventListener("keydown", handleKeyDown);
      return () => document.removeEventListener("keydown", handleKeyDown);
    }, [isOpen, onClose]);
    y$1(() => {
      if (isOpen) {
        document.body.style.overflow = "hidden";
      } else {
        document.body.style.overflow = "";
      }
      return () => {
        document.body.style.overflow = "";
      };
    }, [isOpen]);
    if (!shouldRender) return null;
    return /* @__PURE__ */ u(k$2, { children: [
      /* @__PURE__ */ u(
        "div",
        {
          className: `simple-sheet-overlay ${isAnimating ? "visible" : ""}`,
          onClick: onClose
        }
      ),
      /* @__PURE__ */ u(
        "div",
        {
          ref: sheetRef,
          className: `simple-sheet ${isAnimating ? "open" : ""}`,
          role: "dialog",
          "aria-modal": "true",
          "aria-label": "Product details",
          children
        }
      )
    ] });
  };
  function g(n2, t2) {
    for (var e2 in t2) n2[e2] = t2[e2];
    return n2;
  }
  function E(n2, t2) {
    for (var e2 in n2) if ("__source" !== e2 && !(e2 in t2)) return true;
    for (var r2 in t2) if ("__source" !== r2 && n2[r2] !== t2[r2]) return true;
    return false;
  }
  function N(n2, t2) {
    this.props = n2, this.context = t2;
  }
  function M(n2, e2) {
    function r2(n3) {
      var t2 = this.props.ref, r3 = t2 == n3.ref;
      return !r3 && t2 && (t2.call ? t2(null) : t2.current = null), e2 ? !e2(this.props, n3) || !r3 : E(this.props, n3);
    }
    function u2(e3) {
      return this.shouldComponentUpdate = r2, _$1(n2, e3);
    }
    return u2.displayName = "Memo(" + (n2.displayName || n2.name) + ")", u2.prototype.isReactComponent = true, u2.__f = true, u2;
  }
  (N.prototype = new x$1()).isPureReactComponent = true, N.prototype.shouldComponentUpdate = function(n2, t2) {
    return E(this.props, n2) || E(this.state, t2);
  };
  var T = l$3.__b;
  l$3.__b = function(n2) {
    n2.type && n2.type.__f && n2.ref && (n2.props.ref = n2.ref, n2.ref = null), T && T(n2);
  };
  var F = l$3.__e;
  l$3.__e = function(n2, t2, e2, r2) {
    if (n2.then) {
      for (var u2, o2 = t2; o2 = o2.__; ) if ((u2 = o2.__c) && u2.__c) return null == t2.__e && (t2.__e = e2.__e, t2.__k = e2.__k), u2.__c(n2, t2);
    }
    F(n2, t2, e2, r2);
  };
  var U = l$3.unmount;
  function V(n2, t2, e2) {
    return n2 && (n2.__c && n2.__c.__H && (n2.__c.__H.__.forEach(function(n3) {
      "function" == typeof n3.__c && n3.__c();
    }), n2.__c.__H = null), null != (n2 = g({}, n2)).__c && (n2.__c.__P === e2 && (n2.__c.__P = t2), n2.__c.__e = true, n2.__c = null), n2.__k = n2.__k && n2.__k.map(function(n3) {
      return V(n3, t2, e2);
    })), n2;
  }
  function W(n2, t2, e2) {
    return n2 && e2 && (n2.__v = null, n2.__k = n2.__k && n2.__k.map(function(n3) {
      return W(n3, t2, e2);
    }), n2.__c && n2.__c.__P === t2 && (n2.__e && e2.appendChild(n2.__e), n2.__c.__e = true, n2.__c.__P = e2)), n2;
  }
  function P() {
    this.__u = 0, this.o = null, this.__b = null;
  }
  function j(n2) {
    var t2 = n2.__.__c;
    return t2 && t2.__a && t2.__a(n2);
  }
  function B() {
    this.i = null, this.l = null;
  }
  l$3.unmount = function(n2) {
    var t2 = n2.__c;
    t2 && t2.__R && t2.__R(), t2 && 32 & n2.__u && (n2.type = null), U && U(n2);
  }, (P.prototype = new x$1()).__c = function(n2, t2) {
    var e2 = t2.__c, r2 = this;
    null == r2.o && (r2.o = []), r2.o.push(e2);
    var u2 = j(r2.__v), o2 = false, i2 = function() {
      o2 || (o2 = true, e2.__R = null, u2 ? u2(l2) : l2());
    };
    e2.__R = i2;
    var l2 = function() {
      if (!--r2.__u) {
        if (r2.state.__a) {
          var n3 = r2.state.__a;
          r2.__v.__k[0] = W(n3, n3.__c.__P, n3.__c.__O);
        }
        var t3;
        for (r2.setState({ __a: r2.__b = null }); t3 = r2.o.pop(); ) t3.forceUpdate();
      }
    };
    r2.__u++ || 32 & t2.__u || r2.setState({ __a: r2.__b = r2.__v.__k[0] }), n2.then(i2, i2);
  }, P.prototype.componentWillUnmount = function() {
    this.o = [];
  }, P.prototype.render = function(n2, e2) {
    if (this.__b) {
      if (this.__v.__k) {
        var r2 = document.createElement("div"), o2 = this.__v.__k[0].__c;
        this.__v.__k[0] = V(this.__b, r2, o2.__O = o2.__P);
      }
      this.__b = null;
    }
    var i2 = e2.__a && _$1(k$2, null, n2.fallback);
    return i2 && (i2.__u &= -33), [_$1(k$2, null, e2.__a ? null : n2.children), i2];
  };
  var H = function(n2, t2, e2) {
    if (++e2[1] === e2[0] && n2.l.delete(t2), n2.props.revealOrder && ("t" !== n2.props.revealOrder[0] || !n2.l.size)) for (e2 = n2.i; e2; ) {
      for (; e2.length > 3; ) e2.pop()();
      if (e2[1] < e2[0]) break;
      n2.i = e2 = e2[2];
    }
  };
  (B.prototype = new x$1()).__a = function(n2) {
    var t2 = this, e2 = j(t2.__v), r2 = t2.l.get(n2);
    return r2[0]++, function(u2) {
      var o2 = function() {
        t2.props.revealOrder ? (r2.push(u2), H(t2, n2, r2)) : u2();
      };
      e2 ? e2(o2) : o2();
    };
  }, B.prototype.render = function(n2) {
    this.i = null, this.l = /* @__PURE__ */ new Map();
    var t2 = H$1(n2.children);
    n2.revealOrder && "b" === n2.revealOrder[0] && t2.reverse();
    for (var e2 = t2.length; e2--; ) this.l.set(t2[e2], this.i = [1, 0, this.i]);
    return n2.children;
  }, B.prototype.componentDidUpdate = B.prototype.componentDidMount = function() {
    var n2 = this;
    this.l.forEach(function(t2, e2) {
      H(n2, e2, t2);
    });
  };
  var q = "undefined" != typeof Symbol && Symbol.for && Symbol.for("react.element") || 60103, G = /^(?:accent|alignment|arabic|baseline|cap|clip(?!PathU)|color|dominant|fill|flood|font|glyph(?!R)|horiz|image(!S)|letter|lighting|marker(?!H|W|U)|overline|paint|pointer|shape|stop|strikethrough|stroke|text(?!L)|transform|underline|unicode|units|v|vector|vert|word|writing|x(?!C))[A-Z]/, J = /^on(Ani|Tra|Tou|BeforeInp|Compo)/, K = /[A-Z0-9]/g, Q = "undefined" != typeof document, X = function(n2) {
    return ("undefined" != typeof Symbol && "symbol" == typeof Symbol() ? /fil|che|rad/ : /fil|che|ra/).test(n2);
  };
  x$1.prototype.isReactComponent = {}, ["componentWillMount", "componentWillReceiveProps", "componentWillUpdate"].forEach(function(t2) {
    Object.defineProperty(x$1.prototype, t2, { configurable: true, get: function() {
      return this["UNSAFE_" + t2];
    }, set: function(n2) {
      Object.defineProperty(this, t2, { configurable: true, writable: true, value: n2 });
    } });
  });
  var en = l$3.event;
  function rn() {
  }
  function un() {
    return this.cancelBubble;
  }
  function on() {
    return this.defaultPrevented;
  }
  l$3.event = function(n2) {
    return en && (n2 = en(n2)), n2.persist = rn, n2.isPropagationStopped = un, n2.isDefaultPrevented = on, n2.nativeEvent = n2;
  };
  var cn = { enumerable: false, configurable: true, get: function() {
    return this.class;
  } }, fn = l$3.vnode;
  l$3.vnode = function(n2) {
    "string" == typeof n2.type && function(n3) {
      var t2 = n3.props, e2 = n3.type, u2 = {}, o2 = -1 === e2.indexOf("-");
      for (var i2 in t2) {
        var l2 = t2[i2];
        if (!("value" === i2 && "defaultValue" in t2 && null == l2 || Q && "children" === i2 && "noscript" === e2 || "class" === i2 || "className" === i2)) {
          var c2 = i2.toLowerCase();
          "defaultValue" === i2 && "value" in t2 && null == t2.value ? i2 = "value" : "download" === i2 && true === l2 ? l2 = "" : "translate" === c2 && "no" === l2 ? l2 = false : "o" === c2[0] && "n" === c2[1] ? "ondoubleclick" === c2 ? i2 = "ondblclick" : "onchange" !== c2 || "input" !== e2 && "textarea" !== e2 || X(t2.type) ? "onfocus" === c2 ? i2 = "onfocusin" : "onblur" === c2 ? i2 = "onfocusout" : J.test(i2) && (i2 = c2) : c2 = i2 = "oninput" : o2 && G.test(i2) ? i2 = i2.replace(K, "-$&").toLowerCase() : null === l2 && (l2 = void 0), "oninput" === c2 && u2[i2 = c2] && (i2 = "oninputCapture"), u2[i2] = l2;
        }
      }
      "select" == e2 && u2.multiple && Array.isArray(u2.value) && (u2.value = H$1(t2.children).forEach(function(n4) {
        n4.props.selected = -1 != u2.value.indexOf(n4.props.value);
      })), "select" == e2 && null != u2.defaultValue && (u2.value = H$1(t2.children).forEach(function(n4) {
        n4.props.selected = u2.multiple ? -1 != u2.defaultValue.indexOf(n4.props.value) : u2.defaultValue == n4.props.value;
      })), t2.class && !t2.className ? (u2.class = t2.class, Object.defineProperty(u2, "className", cn)) : (t2.className && !t2.class || t2.class && t2.className) && (u2.class = u2.className = t2.className), n3.props = u2;
    }(n2), n2.$$typeof = q, fn && fn(n2);
  };
  var an = l$3.__r;
  l$3.__r = function(n2) {
    an && an(n2), n2.__c;
  };
  var sn = l$3.diffed;
  l$3.diffed = function(n2) {
    sn && sn(n2);
    var t2 = n2.props, e2 = n2.__e;
    null != e2 && "textarea" === n2.type && "value" in t2 && t2.value !== e2.value && (e2.value = null == t2.value ? "" : t2.value);
  };
  const ImageComponent = ({
    imageUrl,
    alt,
    children,
    onClick
  }) => {
    const [isLoaded, setIsLoaded] = d$2(false);
    const [hasError, setHasError] = d$2(false);
    const handleLoad = q$1(() => {
      setIsLoaded(true);
    }, []);
    const handleError = q$1(() => {
      setHasError(true);
      setIsLoaded(false);
    }, []);
    return /* @__PURE__ */ u("div", { className: "product-cart__image-wrapper", onClick, children: [
      imageUrl && !hasError && /* @__PURE__ */ u(
        "img",
        {
          className: `product-cart__image ${isLoaded ? "product-cart__image--loaded" : "product-cart__image--loading"}`,
          src: imageUrl,
          alt,
          onLoad: handleLoad,
          onError: handleError,
          loading: "lazy"
        }
      ),
      /* @__PURE__ */ u("div", { className: `product-cart__image-placeholder ${isLoaded && !hasError ? "product-cart__image-placeholder--loaded" : "product-cart__image-placeholder--loading"}`, children: hasError ? /* @__PURE__ */ u("div", { style: { color: "#666", fontSize: "14px", textAlign: "center" }, children: [
        "Изображение",
        /* @__PURE__ */ u("br", {}),
        "не загружено"
      ] }) : /* @__PURE__ */ u("div", { style: { color: "#666" }, children: "Загрузка..." }) }),
      children
    ] });
  };
  function formatQuantity(value, unit) {
    if (unit === "g") {
      if (value < 1e3) {
        return `${value} г`;
      } else {
        const kgValue = value / 1e3;
        const rounded = Math.round(kgValue * 100) / 100;
        return `${rounded} кг`;
      }
    }
    if (unit === "ml") {
      if (value < 1e3) {
        return `${value} мл`;
      } else {
        const lValue = value / 1e3;
        const rounded = Math.round(lValue * 100) / 100;
        return `${rounded} л`;
      }
    }
    return `${value} ${unit}`;
  }
  const QuantityControl = ({
    basePrice,
    quantity,
    onQuantityChange,
    unit,
    step,
    discounts = []
    // Значение по умолчанию для необязательного параметра
  }) => {
    const [localQuantity, setLocalQuantity] = d$2(quantity);
    y$1(() => {
      setLocalQuantity(quantity);
    }, [quantity]);
    const increase = q$1(() => {
      const newQuantity = localQuantity + step;
      setLocalQuantity(newQuantity);
      onQuantityChange(newQuantity);
    }, [localQuantity, step, onQuantityChange]);
    const decrease = q$1(() => {
      const newQuantity = localQuantity > step ? localQuantity - step : step;
      setLocalQuantity(newQuantity);
      onQuantityChange(newQuantity);
    }, [localQuantity, step, onQuantityChange]);
    T$1(() => {
      let price = basePrice;
      if (discounts && discounts.length > 0) {
        const totalQuantityInSteps = localQuantity / step;
        const applicableDiscounts = discounts.filter(
          (discount) => totalQuantityInSteps >= discount.quantity
        );
        if (applicableDiscounts.length > 0) {
          const maxDiscount = applicableDiscounts.reduce((prev, curr) => {
            return curr.quantity > prev.quantity ? curr : prev;
          });
          price = maxDiscount.price;
        }
      }
      return Number(price);
    }, [basePrice, localQuantity, discounts, step]);
    const formattedQuantity = T$1(() => {
      return formatQuantity(localQuantity, unit);
    }, [localQuantity, unit]);
    return /* @__PURE__ */ u("div", { className: "quantity-control", children: [
      /* @__PURE__ */ u(
        "button",
        {
          className: "quantity-control__button",
          onClick: decrease,
          "aria-label": "Уменьшить количество",
          children: /* @__PURE__ */ u("span", { children: "-" })
        }
      ),
      /* @__PURE__ */ u("div", { className: "quantity-control__quantity", children: formattedQuantity }),
      /* @__PURE__ */ u(
        "button",
        {
          className: "quantity-control__button",
          onClick: increase,
          "aria-label": "Увеличить количество",
          children: /* @__PURE__ */ u("span", { children: "+" })
        }
      )
    ] });
  };
  const QuantityControl$1 = M(QuantityControl);
  const Price = ({
    amount,
    insideButton = false,
    isActive = false,
    fontSize,
    detailView = false
  }) => {
    const getAmountClass = () => {
      let classes = ["price__amount"];
      if (insideButton) {
        classes.push("price__amount--inside-button");
        if (isActive) {
          classes.push("price__amount--inside-button-active");
        } else {
          classes.push("price__amount--inside-button-inactive");
        }
      } else {
        classes.push("price__amount--default");
      }
      if (detailView) {
        if (insideButton) {
          classes.push("price__amount--detail-view-inside-button");
        } else {
          classes.push("price__amount--detail-view");
        }
      }
      return classes.join(" ");
    };
    const containerClass = insideButton ? "price price--inside-button" : "price price--outside-button";
    return /* @__PURE__ */ u("div", { className: containerClass, children: /* @__PURE__ */ u(
      "span",
      {
        className: getAmountClass(),
        style: fontSize ? { fontSize } : void 0,
        children: [
          amount,
          " ₾"
        ]
      }
    ) });
  };
  const ToggleButton = ({
    onClick,
    isActive,
    isDisabled,
    price,
    detailView = false,
    fontSize,
    priceFontSize,
    outOfStock = false,
    vendorClosed = false,
    children
  }) => {
    const isUnavailable = outOfStock || vendorClosed;
    const getButtonClass = () => {
      let classes = ["toggle-button"];
      if (isUnavailable) {
        if (outOfStock) {
          classes.push("toggle-button--out-of-stock");
        } else if (vendorClosed) {
          classes.push("toggle-button--vendor-closed");
        }
      } else if (isActive) {
        classes.push("toggle-button--active");
      }
      if (isDisabled || isUnavailable) {
        classes.push("toggle-button--disabled");
      }
      return classes.join(" ");
    };
    const getButtonText = () => {
      if (outOfStock) {
        return "Нет в наличии";
      }
      if (vendorClosed) {
        return "Магазин закрыт";
      }
      if (children) {
        return children;
      }
      return isActive ? "Убрать из корзины" : "Добавить в корзину";
    };
    return /* @__PURE__ */ u(
      "button",
      {
        className: getButtonClass(),
        onClick: isUnavailable ? () => {
        } : onClick,
        disabled: isDisabled || isUnavailable,
        style: fontSize ? { fontSize } : void 0,
        "aria-label": getButtonText(),
        children: [
          price !== void 0 && /* @__PURE__ */ u(
            Price,
            {
              amount: price,
              insideButton: true,
              isActive: isUnavailable ? false : isActive,
              fontSize: priceFontSize || (detailView ? "18px" : void 0),
              detailView
            }
          ),
          getButtonText()
        ]
      }
    );
  };
  const productCache = {};
  const useProductDetails = (productId) => {
    const [loading2, setLoading] = d$2(false);
    const [error, setError] = d$2(null);
    const [productDetails, setProductDetails] = d$2(null);
    const isMountedRef = A(true);
    const fetchProductDetails = q$1(async () => {
      if (!productId) {
        setLoading(false);
        setError(null);
        setProductDetails(null);
        return;
      }
      if (!isMountedRef.current) return;
      if (productCache[productId]) {
        setProductDetails(productCache[productId]);
        setLoading(false);
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const token = localStorage.getItem("token");
        const headers = {
          "Content-Type": "application/json"
        };
        if (token) {
          headers["Authorization"] = `Bearer ${token}`;
        }
        const response = await fetch(`/api/products/${productId}`, {
          method: "GET",
          headers,
          credentials: "include"
        });
        if (!response.ok) {
          throw new Error(`API request failed: ${response.status}`);
        }
        const data = await response.json();
        productCache[productId] = data;
        if (isMountedRef.current) {
          setProductDetails(data);
          setError(null);
        }
      } catch (err) {
        if (isMountedRef.current) {
          const errorMessage = err instanceof Error ? err.message : "Network error";
          setError(errorMessage);
          console.error("Failed to fetch product details:", errorMessage);
        }
      } finally {
        if (isMountedRef.current) {
          setLoading(false);
        }
      }
    }, [productId]);
    const retry = q$1(() => {
      if (productId && productCache[productId]) {
        delete productCache[productId];
      }
      fetchProductDetails();
    }, [fetchProductDetails, productId]);
    y$1(() => {
      isMountedRef.current = true;
      fetchProductDetails();
      return () => {
        isMountedRef.current = false;
      };
    }, [fetchProductDetails]);
    return { loading: loading2, error, productDetails, retry };
  };
  const CardPlaceholder = ({ count = 8 }) => {
    return /* @__PURE__ */ u(k$2, { children: Array.from({ length: count }).map((_2, index) => /* @__PURE__ */ u("div", { className: "card-placeholder", children: [
      /* @__PURE__ */ u("div", { className: "card-placeholder__image" }),
      /* @__PURE__ */ u("div", { className: "card-placeholder__title" }),
      /* @__PURE__ */ u("div", { className: "card-placeholder__price" }),
      /* @__PURE__ */ u("div", { className: "card-placeholder__button" })
    ] }, index)) });
  };
  const DetailPlaceholder = () => {
    return /* @__PURE__ */ u("div", { className: "detail-placeholder", children: [
      /* @__PURE__ */ u("div", { className: "detail-placeholder__section", children: /* @__PURE__ */ u("div", { className: "detail-placeholder__image" }) }),
      /* @__PURE__ */ u("div", { className: "detail-placeholder__section", children: [
        /* @__PURE__ */ u("div", { className: "detail-placeholder__title" }),
        /* @__PURE__ */ u("div", { className: "detail-placeholder__paragraph" }),
        /* @__PURE__ */ u("div", { className: "detail-placeholder__paragraph detail-placeholder__paragraph--medium" }),
        /* @__PURE__ */ u("div", { className: "detail-placeholder__paragraph detail-placeholder__paragraph--short" })
      ] }),
      /* @__PURE__ */ u("div", { className: "detail-placeholder__section", children: [
        /* @__PURE__ */ u("div", { className: "detail-placeholder__title" }),
        /* @__PURE__ */ u("div", { className: "detail-placeholder__grid", children: [
          /* @__PURE__ */ u("div", { className: "detail-placeholder__item" }),
          /* @__PURE__ */ u("div", { className: "detail-placeholder__item" }),
          /* @__PURE__ */ u("div", { className: "detail-placeholder__item" }),
          /* @__PURE__ */ u("div", { className: "detail-placeholder__item" })
        ] })
      ] }),
      /* @__PURE__ */ u("div", { className: "detail-placeholder__section", children: [
        /* @__PURE__ */ u("div", { className: "detail-placeholder__paragraph" }),
        /* @__PURE__ */ u("div", { className: "detail-placeholder__paragraph detail-placeholder__paragraph--medium" })
      ] })
    ] });
  };
  const calculateTotalPrice = (basePrice, quantity, unit, step, discounts) => {
    let price = basePrice;
    if (discounts && discounts.length > 0) {
      const quantityInSteps = unit === "g" ? quantity / step : quantity;
      const applicableDiscounts = discounts.filter((discount) => quantityInSteps >= discount.quantity);
      if (applicableDiscounts.length > 0) {
        const maxDiscount = applicableDiscounts.reduce(
          (prev, curr) => curr.price < prev.price ? curr : prev
        );
        price = maxDiscount.price;
      }
    }
    return unit === "g" ? Math.round(price * quantity / step * 100) / 100 : Math.round(price * quantity * 100) / 100;
  };
  const ProductDetailView = ({
    product,
    onClose,
    onAddToCart,
    isVendorOpen = true,
    t: t2 = (key, fallback) => fallback || key,
    language
  }) => {
    const [quantity, setQuantity] = d$2(product.step);
    const [isCopied, setIsCopied] = d$2(false);
    const { loading: loading2, error, productDetails } = useProductDetails(product.id);
    const cartItem = getCartItemById(product.id);
    const currentLang = language || currentLanguage.value;
    const actualStockQuantity = T$1(() => getProductStock(product.id), [product.id]);
    const isInCart = !!cartItem;
    const currentQuantity = isInCart ? cartItem.quantity : quantity;
    const localizedTitle = T$1(() => {
      if (!product.titles) {
        return `Product ${product.id}`;
      }
      return getLocalizedTitle(product.titles, currentLang);
    }, [product.titles, currentLang]);
    const processedImageUrl = T$1(() => {
      const imageUrl = (productDetails == null ? void 0 : productDetails.imageUrl) || product.imageUrl;
      return getImageUrl(imageUrl);
    }, [product.imageUrl, productDetails == null ? void 0 : productDetails.imageUrl]);
    const totalPrice = T$1(
      () => calculateTotalPrice(product.price, currentQuantity, product.unit, product.step, product.discounts || []),
      [product.price, currentQuantity, product.unit, product.step, product.discounts]
    );
    const isActuallyOutOfStock = T$1(() => {
      if (product.is_replenishable) return false;
      return actualStockQuantity !== void 0 && actualStockQuantity >= 0 && actualStockQuantity === 0;
    }, [actualStockQuantity, product.is_replenishable]);
    const isProductUnavailable = isActuallyOutOfStock || !isVendorOpen;
    const productInfo = T$1(() => {
      let stockInfoText = "";
      if (product.is_replenishable) {
        stockInfoText = t2("product.always_in_stock", "Всегда в наличии");
      } else if (isActuallyOutOfStock) {
        stockInfoText = t2("product.out_of_stock", "Нет в наличии");
      } else if (actualStockQuantity !== void 0 && actualStockQuantity < 0) {
        stockInfoText = t2("product.always_in_stock", "Всегда в наличии");
      } else {
        const unitText = product.unit === "kg" ? "кг" : product.unit === "g" ? "г" : "шт.";
        stockInfoText = `${t2("product.in_stock", "В наличии")}: ${actualStockQuantity} ${unitText}`;
      }
      return {
        actualStockQuantity,
        isActuallyOutOfStock,
        isProductUnavailable,
        localizedTitle,
        totalPrice,
        stockInfo: stockInfoText,
        description: !loading2 && !error && (productDetails == null ? void 0 : productDetails.description) ? productDetails.description : `${localizedTitle}. ${t2("product.suitable_for", "Подойдёт для супов, салатов и вторых блюд.")}`,
        nutritionInfo: {
          calories: !loading2 && !error && (productDetails == null ? void 0 : productDetails.calories) !== void 0 ? productDetails.calories : 69,
          proteins: !loading2 && !error && (productDetails == null ? void 0 : productDetails.proteins) !== void 0 ? `${productDetails.proteins} ${t2("product.g", "г")}` : `1.6 ${t2("product.g", "г")}`,
          fats: !loading2 && !error && (productDetails == null ? void 0 : productDetails.fats) !== void 0 ? `${productDetails.fats} ${t2("product.g", "г")}` : `0.1 ${t2("product.g", "г")}`,
          carbs: !loading2 && !error && (productDetails == null ? void 0 : productDetails.carbs) !== void 0 ? `${productDetails.carbs} ${t2("product.g", "г")}` : `13.3 ${t2("product.g", "г")}`
        },
        storageInfo: {
          shelfLife: !loading2 && !error && (productDetails == null ? void 0 : productDetails.shelf_life) ? productDetails.shelf_life : t2("product.one_month", "1 месяц"),
          storageConditions: !loading2 && !error && (productDetails == null ? void 0 : productDetails.storage_conditions) ? productDetails.storage_conditions : t2("product.temperature_range", "При температуре от +2°C до +6°C")
        },
        manufacturerInfo: !loading2 && !error && (productDetails == null ? void 0 : productDetails.manufacturer) ? productDetails.manufacturer : null,
        compositionInfo: !loading2 && !error && (productDetails == null ? void 0 : productDetails.composition) ? productDetails.composition : null
      };
    }, [
      actualStockQuantity,
      isActuallyOutOfStock,
      isVendorOpen,
      localizedTitle,
      totalPrice,
      t2,
      product.unit,
      product.is_replenishable,
      loading2,
      error,
      productDetails
    ]);
    const handleQuantityChange = q$1((newQuantity) => {
      if (cartItem) {
        updateCartItem({ ...cartItem, quantity: newQuantity });
      } else {
        setQuantity(newQuantity);
      }
    }, [cartItem]);
    const handleToggleCart = q$1(() => {
      if (isProductUnavailable) return;
      if (isInCart) {
        removeItemFromCart(product.id);
      } else {
        const cartItemData = {
          id: product.id,
          title: localizedTitle,
          titles: product.titles || { en: localizedTitle, ru: localizedTitle, geo: localizedTitle },
          price: product.price,
          quantity: currentQuantity,
          unit: product.unit,
          step: product.step,
          discounts: product.discounts || []
        };
        addItemToCart(cartItemData);
      }
      if (onAddToCart) {
        onAddToCart();
      }
    }, [isProductUnavailable, isInCart, product, localizedTitle, currentQuantity, onAddToCart]);
    const handleShare = q$1(() => {
      if (isCopied) return;
      const url = `${window.location.origin}/${product.slug || product.id}`;
      navigator.clipboard.writeText(url).then(() => {
        setIsCopied(true);
        console.log("Ссылка скопирована!");
        setTimeout(() => setIsCopied(false), 2e3);
      }).catch((err) => {
        console.error("Не удалось скопировать ссылку: ", err);
      });
    }, [product.slug, product.id, isCopied]);
    if (loading2 && !productDetails) {
      return /* @__PURE__ */ u("div", { className: "product-detail-view", children: /* @__PURE__ */ u(DetailPlaceholder, {}) });
    }
    return /* @__PURE__ */ u("div", { className: "product-detail-view", children: [
      onClose && /* @__PURE__ */ u("div", { className: "product-detail-view__header", children: /* @__PURE__ */ u(
        "button",
        {
          className: "product-detail-view__close-btn",
          onClick: () => {
            console.log("Close button clicked in ProductDetailView");
            onClose();
          },
          type: "button",
          children: "×"
        }
      ) }),
      /* @__PURE__ */ u("div", { className: "product-detail-view__content", children: [
        /* @__PURE__ */ u("div", { className: "product-detail-view__image-container", children: [
          /* @__PURE__ */ u(
            ImageComponent,
            {
              imageUrl: processedImageUrl,
              alt: productInfo.localizedTitle
            }
          ),
          productInfo.isProductUnavailable && /* @__PURE__ */ u("div", { className: "product-detail-view__unavailable-overlay", children: /* @__PURE__ */ u("div", { className: `product-detail-view__unavailable-badge ${productInfo.isActuallyOutOfStock ? "product-detail-view__unavailable-badge--out-of-stock" : "product-detail-view__unavailable-badge--vendor-closed"}`, children: productInfo.isActuallyOutOfStock ? t2("product.out_of_stock", "Нет в наличии") : t2("vendor.closed", "Магазин закрыт") }) })
        ] }),
        /* @__PURE__ */ u("div", { className: "product-detail-view__info", children: [
          /* @__PURE__ */ u("div", { className: "product-detail-view__header-section", children: [
            /* @__PURE__ */ u("h1", { className: "product-detail-view__title", children: productInfo.localizedTitle }),
            /* @__PURE__ */ u(
              "button",
              {
                className: "product-detail-view__share-btn",
                onClick: handleShare,
                type: "button",
                children: [
                  /* @__PURE__ */ u("span", { className: "product-detail-view__share-icon", children: "↗" }),
                  isCopied ? t2("product.copied", "Скопировано!") : t2("product.share", "Поделиться")
                ]
              }
            )
          ] }),
          /* @__PURE__ */ u("div", { className: `product-detail-view__stock-info ${!productInfo.isActuallyOutOfStock ? "product-detail-view__stock-info--in-stock" : "product-detail-view__stock-info--out-of-stock"}`, children: productInfo.stockInfo }),
          /* @__PURE__ */ u("p", { className: "product-detail-view__description", children: productInfo.description }),
          /* @__PURE__ */ u("div", { className: "product-detail-view__nutrition-section", children: [
            /* @__PURE__ */ u("div", { className: "product-detail-view__section-label", children: t2("product.nutrition_per_100g", "В 100 граммах") }),
            /* @__PURE__ */ u("div", { className: "product-detail-view__nutrition-grid", children: [
              /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-box", children: [
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-value", children: productInfo.nutritionInfo.calories }),
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-label", children: t2("product.calories", "Ккал") })
              ] }),
              /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-box", children: [
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-value", children: productInfo.nutritionInfo.proteins }),
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-label", children: t2("product.protein", "Белки") })
              ] }),
              /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-box", children: [
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-value", children: productInfo.nutritionInfo.fats }),
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-label", children: t2("product.fat", "Жиры") })
              ] }),
              /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-box", children: [
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-value", children: productInfo.nutritionInfo.carbs }),
                /* @__PURE__ */ u("div", { className: "product-detail-view__nutrient-label", children: t2("product.carbs", "Углеводы") })
              ] })
            ] })
          ] }),
          /* @__PURE__ */ u("div", { className: "product-detail-view__storage-section", children: [
            /* @__PURE__ */ u("div", { className: "product-detail-view__storage-item", children: [
              /* @__PURE__ */ u("div", { className: "product-detail-view__storage-label", children: t2("product.shelf_life", "Срок хранения") }),
              /* @__PURE__ */ u("div", { className: "product-detail-view__storage-value", children: productInfo.storageInfo.shelfLife })
            ] }),
            /* @__PURE__ */ u("div", { className: "product-detail-view__storage-item", children: [
              /* @__PURE__ */ u("div", { className: "product-detail-view__storage-label", children: t2("product.storage_conditions", "Условия хранения") }),
              /* @__PURE__ */ u("div", { className: "product-detail-view__storage-value", children: productInfo.storageInfo.storageConditions })
            ] })
          ] }),
          productInfo.manufacturerInfo && /* @__PURE__ */ u("div", { className: "product-detail-view__manufacturer-section", children: [
            /* @__PURE__ */ u("div", { className: "product-detail-view__manufacturer-label", children: t2("product.manufacturer", "Производитель") }),
            /* @__PURE__ */ u("div", { className: "product-detail-view__manufacturer-value", children: productInfo.manufacturerInfo })
          ] }),
          productInfo.compositionInfo && /* @__PURE__ */ u("div", { className: "product-detail-view__composition-section", children: [
            /* @__PURE__ */ u("div", { className: "product-detail-view__composition-label", children: t2("product.composition", "Состав") }),
            /* @__PURE__ */ u("div", { className: "product-detail-view__composition-value", children: productInfo.compositionInfo })
          ] }),
          /* @__PURE__ */ u("div", { className: "product-detail-view__action-section", children: /* @__PURE__ */ u("div", { className: "product-detail-view__controls", children: [
            /* @__PURE__ */ u(
              QuantityControl$1,
              {
                basePrice: product.price,
                quantity: currentQuantity,
                onQuantityChange: handleQuantityChange,
                unit: product.unit,
                step: product.step,
                discounts: product.discounts
              }
            ),
            /* @__PURE__ */ u(
              ToggleButton,
              {
                onClick: handleToggleCart,
                isActive: isInCart,
                isDisabled: productInfo.isProductUnavailable,
                price: productInfo.totalPrice,
                detailView: true,
                fontSize: "20px",
                priceFontSize: "18px",
                outOfStock: productInfo.isActuallyOutOfStock,
                vendorClosed: !isVendorOpen,
                children: isInCart ? t2("remove_from_cart", "Убрать из корзины") : t2("add_to_cart", "Добавить в корзину")
              }
            )
          ] }) })
        ] })
      ] })
    ] });
  };
  const ProductDetailView$1 = M(ProductDetailView);
  const Basket = () => {
    return _$1(
      "div",
      { className: "basket-inner" },
      // Header
      _$1("h2", { className: "basket-title" }, "Корзина"),
      // Empty state placeholder
      _$1(
        "div",
        { className: "basket-empty" },
        _$1("div", { className: "basket-empty-icon" }, "🛒"),
        _$1("p", { className: "basket-empty-text" }, "Корзина пуста"),
        _$1("p", { className: "basket-empty-subtext" }, "Добавьте товары из каталога")
      )
    );
  };
  const Authorization = ({ isOpen, onClose }) => {
    const [phoneNumber, setPhoneNumber] = d$2("");
    const [code, setCode] = d$2("");
    const [step, setStep] = d$2("phone");
    if (!isOpen) return null;
    const handleSendCode = () => {
      if (phoneNumber) {
        setStep("code");
      }
    };
    const handleVerifyCode = () => {
      console.log("Verifying code:", code);
    };
    const handleBack = () => {
      setStep("phone");
      setCode("");
    };
    return _$1(
      "div",
      { className: "auth-overlay", onClick: onClose },
      _$1(
        "div",
        {
          className: "auth-container",
          onClick: (e2) => e2.stopPropagation()
        },
        // Header
        _$1(
          "div",
          { className: "auth-header" },
          _$1(
            "h2",
            { className: "auth-title" },
            step === "phone" ? "👤 Вход" : "✉️ Код подтверждения"
          ),
          _$1("button", {
            className: "auth-close",
            onClick: onClose,
            "aria-label": "Закрыть"
          }, "×")
        ),
        // Content
        _$1(
          "div",
          { className: "auth-content" },
          step === "phone" ? (
            // Phone step
            _$1(
              "div",
              { className: "auth-step" },
              _$1(
                "p",
                { className: "auth-description" },
                "Введите номер телефона для входа или регистрации"
              ),
              _$1(
                "div",
                { className: "auth-input-group" },
                _$1("label", { className: "auth-label" }, "📱 Номер телефона"),
                _$1("input", {
                  type: "tel",
                  className: "auth-input",
                  placeholder: "+995...",
                  value: phoneNumber,
                  onInput: (e2) => setPhoneNumber(e2.target.value),
                  autofocus: true
                })
              ),
              _$1("button", {
                className: "auth-button",
                onClick: handleSendCode,
                disabled: !phoneNumber
              }, "Получить код")
            )
          ) : (
            // Code step
            _$1(
              "div",
              { className: "auth-step" },
              _$1(
                "p",
                { className: "auth-description" },
                `Код отправлен на номер ${phoneNumber}`
              ),
              _$1(
                "div",
                { className: "auth-input-group" },
                _$1("label", { className: "auth-label" }, "🔢 Код подтверждения"),
                _$1("input", {
                  type: "text",
                  className: "auth-input auth-input-code",
                  placeholder: "••••",
                  value: code,
                  onInput: (e2) => setCode(e2.target.value),
                  maxLength: 4,
                  autofocus: true
                })
              ),
              _$1("button", {
                className: "auth-button",
                onClick: handleVerifyCode,
                disabled: code.length < 4
              }, "Войти"),
              _$1("button", {
                className: "auth-button-secondary",
                onClick: handleBack
              }, "← Назад")
            )
          ),
          // Info text
          _$1(
            "p",
            { className: "auth-info" },
            'Нажимая "Получить код", вы соглашаетесь с условиями использования'
          )
        )
      )
    );
  };
  const isClient = typeof window !== "undefined";
  function useWindowWidth() {
    const [width, setWidth] = d$2(isClient ? window.innerWidth : 1200);
    y$1(() => {
      if (!isClient) return;
      const handleResize = () => setWidth(window.innerWidth);
      window.addEventListener("resize", handleResize);
      return () => window.removeEventListener("resize", handleResize);
    }, []);
    return width;
  }
  function Shop() {
    const [selectedProduct, setSelectedProduct] = d$2(null);
    const [isDetailOpen, setIsDetailOpen] = d$2(false);
    const [isAuthOpen, setIsAuthOpen] = d$2(false);
    const [isMobileMenuOpen, setIsMobileMenuOpen] = d$2(false);
    const windowWidth = useWindowWidth();
    const isDesktop = windowWidth > 652;
    if (isClient) {
      y$1(() => {
        if (products.value.length === 0) {
          loadProducts();
        }
      }, []);
    }
    const currentProducts = isClient ? filteredProducts.value : products.value;
    const currentLoading = isClient ? loading.value : false;
    const currentCartCount = isClient ? cartCount.value : 0;
    isClient ? cartTotal.value : 0;
    const currentHasOrders = isClient ? hasActiveOrders.value : false;
    const currentOrderStatus = isClient ? orderStatusText.value : "";
    const currentSearchQuery = isClient ? searchQuery.value : "";
    const handleAddToCart = (product) => {
      if (!isClient) return;
      if (product.stock_quantity <= 0) {
        alert("Товар закончился");
        return;
      }
      addToCart(product);
    };
    const handleSearch = (e2) => {
      if (!isClient) return;
      const target = e2.target;
      searchQuery.value = target.value;
    };
    const handleProductClick = (product) => {
      if (!isClient) return;
      console.log("Product clicked:", product);
      setSelectedProduct(product);
      setIsDetailOpen(true);
    };
    const handleCloseDetail = () => {
      console.log("Closing detail view");
      setIsDetailOpen(false);
      setTimeout(() => setSelectedProduct(null), 300);
    };
    if (isClient && currentLoading && currentProducts.length === 0) {
      return _$1(
        "div",
        { className: "container" },
        _$1(
          "header",
          { className: "shop-header" },
          _$1("h1", { className: "title" }, "🛍️ Магазин Enddel")
        ),
        _$1(
          "div",
          { className: "products-grid" },
          _$1(CardPlaceholder, { count: 8 })
        )
      );
    }
    return _$1(
      "div",
      { className: "container" },
      // Шапка
      _$1(
        "header",
        { className: "shop-header" },
        _$1("h1", { className: "title" }, "🛍️ Магазин Enddel"),
        _$1(
          "div",
          { className: "shop-controls" },
          _$1(
            "div",
            { className: "search-container" },
            _$1("input", {
              type: "text",
              className: "input input-search",
              placeholder: "Поиск...",
              value: currentSearchQuery,
              onInput: isClient ? handleSearch : void 0,
              disabled: !isClient
            }),
            _$1("span", { className: "search-emoji" }, "🔍")
          ),
          // Кнопка авторизации
          isClient && _$1("button", {
            className: "btn btn-icon",
            onClick: () => setIsAuthOpen(true),
            "aria-label": "Авторизация"
          }, "👤")
        )
      ),
      // Статус заказов
      currentHasOrders && _$1(
        "div",
        {
          className: "order-status-bar",
          style: {
            background: "#e3f2fd",
            padding: "12px",
            borderRadius: "8px",
            margin: "10px 0"
          }
        },
        _$1("span", null, "📋"),
        _$1(
          "div",
          null,
          _$1("strong", null, "Активные заказы: "),
          currentOrderStatus
        )
      ),
      // Showcase: основной контейнер для товаров и корзины
      _$1(
        "div",
        { className: "showcase" },
        // ShopInner: контейнер для товаров
        _$1(
          "div",
          { className: "shop-inner" },
          // ContentWrapper: обертка для контента
          _$1(
            "div",
            { className: "content-wrapper" },
            _$1(
              "div",
              { className: "products-grid" },
              currentProducts.map(
                (product) => _$1(ProductCard, {
                  key: product.id,
                  product,
                  onAddToCart: handleAddToCart,
                  onProductClick: handleProductClick,
                  isClient
                })
              )
            ),
            // Пустое состояние
            currentProducts.length === 0 && _$1(
              "div",
              { className: "empty-state" },
              currentSearchQuery ? `🔍 По запросу "${currentSearchQuery}" ничего не найдено` : "📦 Товары загружаются..."
            )
          )
        ),
        // Десктопная корзина (справа)
        isClient && isDesktop && _$1(
          "div",
          { className: "desktop-sidebar" },
          _$1(Basket),
          _$1(Authorization)
        )
      ),
      // Мобильная кнопка корзины
      isClient && !isDesktop && _$1(
        "button",
        {
          className: "mobile-cart-button",
          onClick: () => setIsMobileMenuOpen(true)
        },
        _$1("span", { className: "cart-icon" }, "🛒"),
        currentCartCount > 0 && _$1("span", { className: "cart-badge" }, currentCartCount.toString())
      ),
      // Мобильное меню с корзиной (bottom sheet)
      isClient && !isDesktop && isMobileMenuOpen && _$1(
        "div",
        {
          className: "mobile-menu-overlay",
          onClick: () => setIsMobileMenuOpen(false)
        },
        _$1(
          "div",
          {
            className: "mobile-menu-content",
            onClick: (e2) => e2.stopPropagation()
          },
          _$1(
            "div",
            { className: "mobile-menu-header" },
            _$1("h2", null, "Меню"),
            _$1("button", {
              className: "mobile-menu-close",
              onClick: () => setIsMobileMenuOpen(false)
            }, "×")
          ),
          _$1(
            "div",
            { className: "mobile-menu-body" },
            _$1(Basket),
            _$1(Authorization)
          )
        )
      ),
      // Bottom Sheet с детальным просмотром товара
      isClient && selectedProduct && _$1(
        SimpleBottomSheet,
        {
          isOpen: isDetailOpen,
          onClose: handleCloseDetail
        },
        _$1(ProductDetailView$1, {
          product: selectedProduct,
          onClose: handleCloseDetail
        })
      ),
      // Авторизация (модал)
      isClient && _$1(Authorization, {
        isOpen: isAuthOpen,
        onClose: () => setIsAuthOpen(false)
      })
    );
  }
  function ProductCard({ product, onAddToCart, onProductClick, isClient: isClient2 }) {
    var _a;
    const inCart = isClient2 ? cartItems.value && Array.isArray(cartItems.value) ? cartItems.value.some((item) => item.id === product.id) : false : false;
    const outOfStock = product.stock_quantity <= 0;
    const title = ((_a = product.name) == null ? void 0 : _a.ru) || product.name || "Товар";
    const imageUrl = product.image_url ? getImageUrl(product.image_url) : null;
    return _$1(
      "div",
      {
        className: "product-card",
        onClick: isClient2 ? () => onProductClick(product) : void 0,
        style: { cursor: "pointer" }
      },
      _$1(
        "div",
        { className: "product-image" },
        imageUrl ? _$1("img", {
          src: imageUrl,
          alt: title,
          loading: "lazy",
          onError: (e2) => {
            if (e2.target.crossOrigin) {
              console.log("Retry without crossOrigin:", imageUrl);
              e2.target.crossOrigin = null;
              e2.target.src = imageUrl;
            } else {
              console.log("Ошибка загрузки изображения, показываем placeholder:", imageUrl);
              e2.target.style.display = "none";
            }
          }
        }) : _$1("div", { className: "product-placeholder" }, "📦"),
        !imageUrl && _$1("div", { className: "product-placeholder" }, "📦"),
        outOfStock && _$1("div", { className: "out-of-stock-badge" }, "Нет в наличии")
      ),
      _$1("h3", { className: "product-name" }, title),
      _$1("div", { className: "product-price" }, `${product.price || 0} ₽`),
      _$1(
        "button",
        {
          className: `btn ${inCart ? "btn-success" : "btn-primary"}`,
          onClick: isClient2 ? (e2) => {
            e2.stopPropagation();
            onAddToCart(product);
          } : void 0,
          disabled: outOfStock || !isClient2
        },
        inCart ? "✓ В корзине" : "В корзину"
      )
    );
  }
  function App() {
    return _$1(Shop, {});
  }
  function normalizeProducts(rawProducts) {
    return rawProducts.map((p2) => ({
      ...p2,
      titles: p2.titles || p2.name
    }));
  }
  function createFallbackProducts() {
    return [{
      id: 1,
      name: { ru: "Тестовый товар", en: "Test Product", geo: "სატესტო პროდუქტი" },
      titles: { ru: "Тестовый товар", en: "Test Product", geo: "სატესტო პროდუქტი" },
      price: 100,
      image_url: null,
      unit: "шт",
      step: 1,
      stock_quantity: 10,
      discounts: [],
      vendor_id: void 0,
      category_id: void 0,
      slug: "test-product"
    }];
  }
  async function renderToString(context) {
    const initialData = {
      products: [],
      url: context.url,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    loading.value = true;
    productsLoading.value = true;
    productsError.value = null;
    try {
      const productsData = context.productsData || [];
      const loadedProducts = Array.isArray(productsData == null ? void 0 : productsData.products) ? productsData.products : Array.isArray(productsData) ? productsData : [];
      const normalized = normalizeProducts(loadedProducts);
      products.value = normalized;
      initialData.products = normalized;
      initialData.totalProducts = normalized.length;
    } catch (error) {
      console.error("Failed to load products for SSR:", error);
      const fallbackProducts = createFallbackProducts();
      products.value = fallbackProducts;
      initialData.products = fallbackProducts;
      initialData.totalProducts = fallbackProducts.length;
      initialData.error = "Failed to load products";
    } finally {
      loading.value = false;
      productsLoading.value = false;
    }
    const html = F$1(_$1(App, {}));
    products.value = [];
    productsError.value = null;
    loading.value = false;
    productsLoading.value = false;
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
globalThis.renderPage = async function(url, productsData) {
    try {
        // Вызываем рендеринг
        const context = {
            url: url,
            headers: {},
            userAgent: 'Rust-V8-SSR/1.0',
            productsData: productsData || []
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
    <script type="module" src="/assets/index-sTKbqqrz.js"></script>
</body>
</html>`;

        return html;
    } catch (error) {
        console.error('SSR Error:', error);
        throw error;
    }
};

console.log('✅ SSR bundle loaded and ready');
