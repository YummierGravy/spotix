use std::{sync::Arc, time::Duration};

use druid::{
    Data, ImageBuf, Point, Selector, TimerToken, WidgetPod,
    widget::{FillStrat, Image, prelude::*},
};

use crate::webapi::WebApi;

pub const REQUEST_DATA: Selector<Arc<str>> = Selector::new("remote-image.request-data");
pub const PROVIDE_DATA: Selector<ImagePayload> = Selector::new("remote-image.provide-data");

#[derive(Clone)]
pub struct ImagePayload {
    pub location: Arc<str>,
    pub image_buf: ImageBuf,
}

pub struct RemoteImage<T> {
    placeholder: WidgetPod<T, Box<dyn Widget<T>>>,
    image: Option<WidgetPod<T, Image>>,
    locator: Box<dyn Fn(&T, &Env) -> Option<Arc<str>>>,
    location: Option<Arc<str>>,
    request_timer: Option<TimerToken>,
    pending_request: Option<Arc<str>>,
}

impl<T: Data> RemoteImage<T> {
    pub fn new(
        placeholder: impl Widget<T> + 'static,
        locator: impl Fn(&T, &Env) -> Option<Arc<str>> + 'static,
    ) -> Self {
        Self {
            placeholder: WidgetPod::new(placeholder).boxed(),
            locator: Box::new(locator),
            location: None,
            image: None,
            request_timer: None,
            pending_request: None,
        }
    }
}

impl<T: Data> Widget<T> for RemoteImage<T> {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        if let Event::Command(cmd) = event
            && let Some(payload) = cmd.get(PROVIDE_DATA)
        {
            if Some(&payload.location) == self.location.as_ref() {
                self.image.replace(WidgetPod::new(
                    Image::new(payload.image_buf.clone()).fill_mode(FillStrat::Cover),
                ));
                ctx.children_changed();
            }
            return;
        }
        if let Event::Timer(token) = event
            && self.request_timer == Some(*token)
        {
            self.request_timer = None;
            if let Some(delay) = WebApi::global().rate_limit_delay() {
                self.request_timer = Some(ctx.request_timer(delay));
                return;
            }
            if let Some(location) = self.pending_request.take()
                && Some(&location) == self.location.as_ref()
            {
                ctx.submit_command(REQUEST_DATA.with(location).to(ctx.widget_id()));
            }
            return;
        }
        if let Some(image) = self.image.as_mut() {
            image.event(ctx, event, data, env);
        } else {
            self.placeholder.event(ctx, event, data, env);
        }
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &T, env: &Env) {
        if let LifeCycle::WidgetAdded = event {
            let location = (self.locator)(data, env);
            self.image = None;
            self.location.clone_from(&location);
            self.pending_request = location;
            if self.pending_request.is_some() {
                let delay = WebApi::global()
                    .rate_limit_delay()
                    .unwrap_or(Duration::from_millis(250));
                self.request_timer = Some(ctx.request_timer(delay));
            }
        }
        if let Some(image) = self.image.as_mut() {
            image.lifecycle(ctx, event, data, env);
        } else {
            self.placeholder.lifecycle(ctx, event, data, env);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &T, data: &T, env: &Env) {
        let location = (self.locator)(data, env);
        if location != self.location {
            self.image = None;
            self.location.clone_from(&location);
            self.pending_request = location;
            if self.pending_request.is_some() {
                let delay = WebApi::global()
                    .rate_limit_delay()
                    .unwrap_or(Duration::from_millis(250));
                self.request_timer = Some(ctx.request_timer(delay));
            } else {
                self.request_timer = None;
            }
            ctx.children_changed();
        }
        if self.request_timer.is_none() && self.pending_request.is_some() {
            let delay = WebApi::global()
                .rate_limit_delay()
                .unwrap_or(Duration::from_millis(250));
            self.request_timer = Some(ctx.request_timer(delay));
        }
        if let Some(image) = self.image.as_mut() {
            image.update(ctx, data, env);
        } else {
            self.placeholder.update(ctx, data, env);
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &T, env: &Env) -> Size {
        if let Some(image) = self.image.as_mut() {
            let size = image.layout(ctx, bc, data, env);
            image.set_origin(ctx, Point::ORIGIN);
            size
        } else {
            let size = self.placeholder.layout(ctx, bc, data, env);
            self.placeholder.set_origin(ctx, Point::ORIGIN);
            size
        }
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &T, env: &Env) {
        if let Some(image) = self.image.as_mut() {
            image.paint(ctx, data, env)
        } else {
            self.placeholder.paint(ctx, data, env)
        }
    }
}
