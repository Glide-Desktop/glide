macro_rules! forward {
    ($field:ident: children) => {
        fn children(&self) -> Vec<iced::advanced::widget::Tree> {
            vec![iced::advanced::widget::Tree::new(&self.$field)]
        }
    };
    ($field:ident: diff) => {
        fn diff(&self, tree: &mut iced::advanced::widget::Tree) {
            tree.diff_children(std::slice::from_ref(&self.$field));
        }
    };
    ($field:ident: size) => {
        fn size(&self) -> iced::Size<iced::Length> {
            self.$field.as_widget().size()
        }
    };
    ($field:ident: size_hint) => {
        fn size_hint(&self) -> iced::Size<iced::Length> {
            self.$field.as_widget().size_hint()
        }
    };
    ($field:ident: layout) => {
        fn layout(
            &mut self, tree: &mut iced::advanced::widget::Tree, renderer: &iced::Renderer,
            limits: &iced::advanced::layout::Limits,
        ) -> iced::advanced::layout::Node {
            self.$field.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
        }
    };
    ($field:ident: operate) => {
        fn operate(
            &mut self, tree: &mut iced::advanced::widget::Tree, layout: iced::advanced::Layout<'_>,
            renderer: &iced::Renderer, operation: &mut dyn iced::advanced::widget::Operation,
        ) {
            self.$field.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
        }
    };
    ($field:ident: draw) => {
        fn draw(
            &self, tree: &iced::advanced::widget::Tree, renderer: &mut iced::Renderer, theme: &iced::Theme,
            style: &iced::advanced::renderer::Style, layout: iced::advanced::Layout<'_>, cursor: iced::mouse::Cursor,
            viewport: &iced::Rectangle,
        ) {
            self.$field.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
        }
    };
    ($field:ident: mouse_interaction) => {
        fn mouse_interaction(
            &self, tree: &iced::advanced::widget::Tree, layout: iced::advanced::Layout<'_>, cursor: iced::mouse::Cursor,
            viewport: &iced::Rectangle, renderer: &iced::Renderer,
        ) -> iced::mouse::Interaction {
            self.$field.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
        }
    };
    ($field:ident: overlay<$message:ident>) => {
        fn overlay<'b>(
            &'b mut self, tree: &'b mut iced::advanced::widget::Tree, layout: iced::advanced::Layout<'b>,
            renderer: &iced::Renderer, viewport: &iced::Rectangle, translation: iced::Vector,
        ) -> Option<iced::advanced::overlay::Element<'b, $message, iced::Theme, iced::Renderer>> {
            self.$field.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
        }
    };
    ($field:ident: $($method:ident $(<$message:ident>)?),+ $(,)?) => {
        $(forward!($field: $method $(<$message>)?);)+
    };
}
