use web_sys::HtmlTextAreaElement;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct EditorProps {
    pub value: AttrValue,
    pub on_change: Callback<String>,
}

#[function_component(Editor)]
pub fn editor(props: &EditorProps) -> Html {
    let on_input = {
        let on_change = props.on_change.clone();
        Callback::from(move |e: InputEvent| {
            let target: HtmlTextAreaElement = e.target_unchecked_into();
            on_change.emit(target.value());
        })
    };

    html! {
        <div class="editor-pane">
            <div class="pane-label">{ "Source" }</div>
            <textarea
                value={props.value.clone()}
                oninput={on_input}
                spellcheck="false"
                placeholder="WaveJSON or sig-int DSL..."
            />
        </div>
    }
}
