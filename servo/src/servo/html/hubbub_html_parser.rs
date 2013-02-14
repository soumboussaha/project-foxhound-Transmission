use content::content_task::ContentTask;
use dom::element::*;
use dom::event::{Event, ReflowEvent};
use dom::node::{AbstractNode, Comment, Doctype, Element, ElementNodeTypeId, Node, Text};
use html::cssparse::{InlineProvenance, StylesheetProvenance, UrlProvenance, spawn_css_parser};
use newcss::stylesheet::Stylesheet;
use resource::image_cache_task::ImageCacheTask;
use resource::image_cache_task;
use resource::resource_task::{Done, Load, Payload, ResourceTask};
use util::task::{spawn_listener, spawn_conversation};

use core::cell::Cell;
use core::comm::{Chan, Port, SharedChan};
use core::str::eq_slice;
use gfx::util::url::make_url;
use hubbub::hubbub::Attribute;
use hubbub::hubbub;
use std::net::url::Url;
use std::net::url;

macro_rules! handle_element(
    ($tag:expr, $string:expr, $ctor:ident, $type_id:expr) => (
        if eq_slice($tag, $string) {
            let element = ~$ctor {
                parent: Element::new($type_id, ($tag).to_str())
            };
            unsafe {
                return Node::as_abstract_node(element);
            }
        }
    )
)

macro_rules! handle_heading_element(
    ($tag:expr, $string:expr, $ctor:ident, $type_id:expr, $level:expr) => (
        if eq_slice($tag, $string) {
            let element = ~HTMLHeadingElement {
                parent: Element::new($type_id, ($tag).to_str()),
                level: $level
            };
            unsafe {
                return Node::as_abstract_node(element);
            }
        }
    )
)

type JSResult = ~[~[u8]];

enum CSSMessage {
    CSSTaskNewFile(StylesheetProvenance),
    CSSTaskExit   
}

enum JSMessage {
    JSTaskNewFile(Url),
    JSTaskExit
}

struct HtmlParserResult {
    root: AbstractNode,
    style_port: Port<Option<Stylesheet>>,
    js_port: Port<JSResult>,
}

trait NodeWrapping {
    unsafe fn to_hubbub_node(self) -> hubbub::NodeDataPtr;
    static unsafe fn from_hubbub_node(n: hubbub::NodeDataPtr) -> Self;
}

impl NodeWrapping for AbstractNode {
    unsafe fn to_hubbub_node(self) -> hubbub::NodeDataPtr {
        cast::transmute(self)
    }
    static unsafe fn from_hubbub_node(n: hubbub::NodeDataPtr) -> AbstractNode {
        cast::transmute(n)
    }
}

/**
Runs a task that coordinates parsing links to css stylesheets.

This function should be spawned in a separate task and spins waiting
for the html builder to find links to css stylesheets and sends off
tasks to parse each link.  When the html process finishes, it notifies
the listener, who then collects the css rules from each task it
spawned, collates them, and sends them to the given result channel.

# Arguments

* `to_parent` - A channel on which to send back the full set of rules.
* `from_parent` - A port on which to receive new links.

*/
fn css_link_listener(to_parent: Chan<Option<Stylesheet>>,
                     from_parent: Port<CSSMessage>,
                     resource_task: ResourceTask) {
    let mut result_vec = ~[];

    loop {
        match from_parent.recv() {
            CSSTaskNewFile(provenance) => {
                result_vec.push(spawn_css_parser(provenance, resource_task.clone()));
            }
            CSSTaskExit => {
                break;
            }
        }
    }

    // Send the sheets back in order
    // FIXME: Shouldn't wait until after we've recieved CSSTaskExit to start sending these
    do vec::consume(result_vec) |_i, port| {
        to_parent.send(Some(port.recv()));
    }
    to_parent.send(None);
}

fn js_script_listener(to_parent: Chan<~[~[u8]]>,
                      from_parent: Port<JSMessage>,
                      resource_task: ResourceTask) {
    let mut result_vec = ~[];

    loop {
        match from_parent.recv() {
            JSTaskNewFile(url) => {
                let (result_port, result_chan) = comm::stream();
                let resource_task = resource_task.clone();
                do task::spawn {
                    let (input_port, input_chan) = comm::stream();
                    // TODO: change copy to move once we can move into closures
                    resource_task.send(Load(copy url, input_chan));

                    let mut buf = ~[];
                    loop {
                        match input_port.recv() {
                            Payload(data) => {
                                buf += data;
                            }
                            Done(Ok(*)) => {
                                result_chan.send(buf);
                                break;
                            }
                            Done(Err(*)) => {
                                error!("error loading script %s", url.to_str());
                            }
                        }
                    }
                }
                vec::push(&mut result_vec, result_port);
            }
            JSTaskExit => {
                break;
            }
        }
    }

    let js_scripts = vec::map(result_vec, |result_port| result_port.recv());
    to_parent.send(js_scripts);
}

// Silly macros to handle constructing DOM nodes. This produces bad code and should be optimized
// via atomization (issue #85).

fn build_element_from_tag(tag: &str) -> AbstractNode {
    // TODO (Issue #85): use atoms
    handle_element!(tag, "a",       HTMLAnchorElement,      HTMLAnchorElementTypeId);
    handle_element!(tag, "aside",   HTMLAsideElement,       HTMLAsideElementTypeId);
    handle_element!(tag, "br",      HTMLBRElement,          HTMLBRElementTypeId);
    handle_element!(tag, "body",    HTMLBodyElement,        HTMLBodyElementTypeId);
    handle_element!(tag, "bold",    HTMLBoldElement,        HTMLBoldElementTypeId);
    handle_element!(tag, "div",     HTMLDivElement,         HTMLDivElementTypeId);
    handle_element!(tag, "font",    HTMLFontElement,        HTMLFontElementTypeId);
    handle_element!(tag, "form",    HTMLFormElement,        HTMLFormElementTypeId);
    handle_element!(tag, "hr",      HTMLHRElement,          HTMLHRElementTypeId);
    handle_element!(tag, "head",    HTMLHeadElement,        HTMLHeadElementTypeId);
    handle_element!(tag, "html",    HTMLHtmlElement,        HTMLHtmlElementTypeId);
    handle_element!(tag, "input",   HTMLInputElement,       HTMLInputElementTypeId);
    handle_element!(tag, "i",       HTMLItalicElement,      HTMLItalicElementTypeId);
    handle_element!(tag, "link",    HTMLLinkElement,        HTMLLinkElementTypeId);
    handle_element!(tag, "li",      HTMLListItemElement,    HTMLListItemElementTypeId);
    handle_element!(tag, "meta",    HTMLMetaElement,        HTMLMetaElementTypeId);
    handle_element!(tag, "ol",      HTMLOListElement,       HTMLOListElementTypeId);
    handle_element!(tag, "option",  HTMLOptionElement,      HTMLOptionElementTypeId);
    handle_element!(tag, "p",       HTMLParagraphElement,   HTMLParagraphElementTypeId);
    handle_element!(tag, "script",  HTMLScriptElement,      HTMLScriptElementTypeId);
    handle_element!(tag, "section", HTMLSectionElement,     HTMLSectionElementTypeId);
    handle_element!(tag, "select",  HTMLSelectElement,      HTMLSelectElementTypeId);
    handle_element!(tag, "small",   HTMLSmallElement,       HTMLSmallElementTypeId);
    handle_element!(tag, "span",    HTMLSpanElement,        HTMLSpanElementTypeId);
    handle_element!(tag, "style",   HTMLStyleElement,       HTMLStyleElementTypeId);
    handle_element!(tag, "tbody",   HTMLTableBodyElement,   HTMLTableBodyElementTypeId);
    handle_element!(tag, "td",      HTMLTableCellElement,   HTMLTableCellElementTypeId);
    handle_element!(tag, "table",   HTMLTableElement,       HTMLTableElementTypeId);
    handle_element!(tag, "tr",      HTMLTableRowElement,    HTMLTableRowElementTypeId);
    handle_element!(tag, "title",   HTMLTitleElement,       HTMLTitleElementTypeId);
    handle_element!(tag, "ul",      HTMLUListElement,       HTMLUListElementTypeId);

    handle_heading_element!(tag, "h1", HTMLHeadingElement, HTMLHeadingElementTypeId, Heading1);
    handle_heading_element!(tag, "h2", HTMLHeadingElement, HTMLHeadingElementTypeId, Heading2);
    handle_heading_element!(tag, "h3", HTMLHeadingElement, HTMLHeadingElementTypeId, Heading3);
    handle_heading_element!(tag, "h4", HTMLHeadingElement, HTMLHeadingElementTypeId, Heading4);
    handle_heading_element!(tag, "h5", HTMLHeadingElement, HTMLHeadingElementTypeId, Heading5);
    handle_heading_element!(tag, "h6", HTMLHeadingElement, HTMLHeadingElementTypeId, Heading6);

    unsafe {
        Node::as_abstract_node(~Element::new(UnknownElementTypeId, tag.to_str()))
    }
}

#[allow(non_implicitly_copyable_typarams)]
pub fn parse_html(url: Url,
                  resource_task: ResourceTask,
                  image_cache_task: ImageCacheTask) -> HtmlParserResult {
    // Spawn a CSS parser to receive links to CSS style sheets.
    let resource_task2 = resource_task.clone();
    let (css_port, css_chan): (Port<Option<Stylesheet>>, Chan<CSSMessage>) =
            do spawn_conversation |css_port: Port<CSSMessage>,
                                   css_chan: Chan<Option<Stylesheet>>| {
        css_link_listener(css_chan, css_port, resource_task2.clone());
    };
    let css_chan = SharedChan(css_chan);

    let resource_task2 = resource_task.clone();
    // Spawn a JS parser to receive JavaScript.
    let resource_task2 = resource_task.clone();
    let (js_port, js_chan): (Port<JSResult>, Chan<JSMessage>) =
            do spawn_conversation |js_port: Port<JSMessage>,
                                   js_chan: Chan<JSResult>| {
        js_script_listener(js_chan, js_port, resource_task2.clone());
    };
    let js_chan = SharedChan(js_chan);

    let url = @url;

    unsafe {
        // Build the root node.
        let root = ~HTMLHtmlElement { parent: Element::new(HTMLHtmlElementTypeId, ~"html") };
        let root = unsafe { Node::as_abstract_node(root) };
        debug!("created new node");
        let mut parser = hubbub::Parser("UTF-8", false);
        debug!("created parser");
        parser.set_document_node(root.to_hubbub_node());
        parser.enable_scripting(true);

        // Performs various actions necessary after appending has taken place. Currently, this
        // consists of processing inline stylesheets, but in the future it might perform
        // prefetching, etc.
        let css_chan2 = css_chan.clone();
        let append_hook: @fn(AbstractNode, AbstractNode) = |parent_node, child_node| {
            if parent_node.is_style_element() && child_node.is_text() {
                debug!("found inline CSS stylesheet");
                let url = url::from_str("http://example.com/"); // FIXME
                let url_cell = Cell(url);
                do child_node.with_imm_text |text_node| {
                    let data = text_node.text.to_str();  // FIXME: Bad copy.
                    let provenance = InlineProvenance(result::unwrap(url_cell.take()), data);
                    css_chan2.send(CSSTaskNewFile(provenance));
                }
            }
        };

        let (css_chan2, js_chan2) = (css_chan.clone(), js_chan.clone());
        parser.set_tree_handler(@hubbub::TreeHandler {
            create_comment: |data: ~str| {
                debug!("create comment");
                unsafe {
                    Node::as_abstract_node(~Comment::new(data)).to_hubbub_node()
                }
            },
            create_doctype: |doctype: ~hubbub::Doctype| {
                debug!("create doctype");
                // TODO: remove copying here by using struct pattern matching to 
                // move all ~strs at once (blocked on Rust #3845, #3846, #3847)
                let public_id = match &doctype.public_id {
                  &None => None,
                  &Some(ref id) => Some(copy *id)
                };
                let system_id = match &doctype.system_id {
                  &None => None,
                  &Some(ref id) => Some(copy *id)
                };
                let node = ~Doctype::new(copy doctype.name,
                                         public_id,
                                         system_id,
                                         doctype.force_quirks);
                unsafe {
                    Node::as_abstract_node(node).to_hubbub_node()
                }
            },
            create_element: |tag: ~hubbub::Tag| {
                debug!("create element");
                // TODO: remove copying here by using struct pattern matching to 
                // move all ~strs at once (blocked on Rust #3845, #3846, #3847)
                let node = build_element_from_tag(tag.name);

                debug!("-- attach attrs");
                do node.as_mut_element |element| {
                    for tag.attributes.each |attr| {
                        element.attrs.push(Attr::new(copy attr.name, copy attr.value));
                    }
                }

                // Spawn additional parsing, network loads, etc. from tag and attrs
                match node.type_id() {
                    // Handle CSS style sheets from <link> elements
                    ElementNodeTypeId(HTMLLinkElementTypeId) => {
                        do node.with_imm_element |element| {
                            match (element.get_attr(~"rel"), element.get_attr(~"href")) {
                                (Some(rel), Some(href)) => {
                                    if rel == ~"stylesheet" {
                                        debug!("found CSS stylesheet: %s", href);
                                        let url = make_url(href.to_str(), Some(copy *url));
                                        css_chan2.send(CSSTaskNewFile(UrlProvenance(url)));
                                    }
                                }
                                _ => {}
                            }
                        }
                    },
                    ElementNodeTypeId(HTMLImageElementTypeId) => {
                        do node.with_mut_image_element |image_element| {
                            let src_opt = image_element.parent.get_attr(~"src").map(|x| x.to_str());
                            match src_opt {
                                None => {}
                                Some(src) => {
                                    let img_url = make_url(src, Some(copy *url));
                                    image_element.image = Some(copy img_url);
                                    // inform the image cache to load this, but don't store a handle.
                                    // TODO (Issue #84): don't prefetch if we are within a <noscript>
                                    // tag.
                                    image_cache_task.send(image_cache_task::Prefetch(img_url));
                                }
                            }
                        }
                    }
                    //TODO (Issue #86): handle inline styles ('style' attr)
                    _ => {}
                }

                unsafe {
                    node.to_hubbub_node()
                }
            },
            create_text: |data: ~str| {
                debug!("create text");
                unsafe {
                    Node::as_abstract_node(~Text::new(data)).to_hubbub_node()
                }
            },
            ref_node: |_| {},
            unref_node: |_| {},
            append_child: |parent: hubbub::NodeDataPtr, child: hubbub::NodeDataPtr| {
                unsafe {
                    debug!("append child %x %x", cast::transmute(parent), cast::transmute(child));
                    let parent: AbstractNode = NodeWrapping::from_hubbub_node(parent);
                    let child: AbstractNode = NodeWrapping::from_hubbub_node(child);
                    parent.append_child(child);
                    append_hook(parent, child);
                }
                child
            },
            insert_before: |_parent, _child| {
                debug!("insert before");
                0u
            },
            remove_child: |_parent, _child| {
                debug!("remove child");
                0u
            },
            clone_node: |node, deep| {
                debug!("clone node");
                unsafe {
                    if deep { error!("-- deep clone unimplemented"); }
                    fail!(~"clone node unimplemented")
                }
            },
            reparent_children: |_node, _new_parent| {
                debug!("reparent children");
                0u
            },
            get_parent: |_node, _element_only| {
                debug!("get parent");
                0u
            },
            has_children: |_node| {
                debug!("has children");
                false
            },
            form_associate: |_form, _node| {
                debug!("form associate");
            },
            add_attributes: |_node, _attributes| {
                debug!("add attributes");
            },
            set_quirks_mode: |_mode| {
                debug!("set quirks mode");
            },
            encoding_change: |_encname| {
                debug!("encoding change");
            },
            complete_script: |script| {
                // A little function for holding this lint attr
                #[allow(non_implicitly_copyable_typarams)]
                fn complete_script(script: hubbub::NodeDataPtr,
                                   url: &Url,
                                   js_chan: SharedChan<JSMessage>) {
                    unsafe {
                        let script: AbstractNode = NodeWrapping::from_hubbub_node(script);
                        do script.with_imm_element |script| {
                            match script.get_attr(~"src") {
                                Some(src) => {
                                    debug!("found script: %s", src);
                                    let new_url = make_url(src.to_str(), Some(copy *url));
                                    js_chan.send(JSTaskNewFile(new_url));
                                }
                                None => {}
                            }
                        }
                    }
                }
                complete_script(script, url, js_chan2.clone());
                debug!("complete script");
            }
        });
        debug!("set tree handler");

        let (input_port, input_chan) = comm::stream();
        resource_task.send(Load(copy *url, input_chan));
        debug!("loaded page");
        loop {
            match input_port.recv() {
                Payload(data) => {
                    debug!("received data");
                    parser.parse_chunk(data);
                }
                Done(*) => {
                    break;
                }
            }
        }

        css_chan.send(CSSTaskExit);
        js_chan.send(JSTaskExit);

        return HtmlParserResult { root: root, style_port: css_port, js_port: js_port };
    }
}

