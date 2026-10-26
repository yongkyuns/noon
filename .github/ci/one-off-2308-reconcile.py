#!/usr/bin/env python3
"""One-off source-pinned reconciliation; #2211 owns removal. No remote writes."""
import os
from pathlib import Path
import re
R=Path(os.environ['RECONCILE_ROOT']);W=R/'work'
pat=re.compile(r'^<<<<<<<[^\n]*\n(.*?)^=======\n(.*?)^>>>>>>>[^\n]*\n',re.M|re.S)
def resolve(path, fn):
 p=W/path;t=p.read_text();i=iter(range(100));p.write_text(pat.sub(lambda m:fn(next(i),m[1],m[2]),t))
def replace(path,old,new,n=1):
 p=W/path;t=p.read_text();assert t.count(old)==n,(path,t.count(old),old[:90]);p.write_text(t.replace(old,new))
# Preserve each side's independent compiler invariants and use the new paint-aware style shape.
resolve('crates/noon-compile/src/lib.rs', lambda i,a,b: a[:a.index('            ExecutionPatch::SetStyle')] + b if i==1 else a+b)
resolve('crates/noon-compile/src/semantic_lowering/publication.rs',lambda i,a,b:'        super::paint::lower_fill(node, &state, prepared.store())?;\n'+a)
resolve('crates/noon-compile/src/transaction_preflight.rs', lambda i,a,b:a+'        }\n    }\n\n'+b)
replace('crates/noon-compile/src/transaction_preflight.rs','                    click_indicate: binding,','                    continuous_fill: crate::continuous_fill_content_supported(&row.content),\n                    click_indicate: binding,')
resolve('crates/noon-runtime/src/transient_animation.rs',lambda i,a,b:a+'    }\n\n'+b)
# Live browser mutation uses the original SceneExecution pair, not a storeless session.
def browser(i,a,b):
 a=a.replace('impl BrowserPointerTarget for ExecutionSession','impl BrowserPointerTarget for noon::SceneExecution')
 a=re.sub(r'(ExecutionSession::\w+)\(self\)',r'\1(self.session())',a)
 a=a.replace('(self, view)', '(self.session(), view)')
 return a
resolve('crates/noon-web/src/browser_pointer_input.rs',browser)
p='crates/noon/src/execution_session/interactions.rs'
t=(R/'master'/p).read_text()
t=t.replace('.and_then(|state| state.click_action());', '.and_then(|state| state.click_action())\n            .filter(|action| !matches!(action, SemanticClickAction::Indicate(_)));')
t=t.replace('        let Some(binding) = self.interaction_bindings.bindings.get(&node) else {\n            return Ok(None);\n        };\n','')
t=t.replace('let Some(object) = self.execution_object_id(node) else {','let Some(object) = self.interaction_execution_object_id(node) else {')
t=t.replace('        match *binding {\n            SemanticClickAction::Indicate(indication) => self\n                .runtime\n                .prepare_click_indicate(object, indication)\n                .map(|animation| animation.map(PreparedClickAction::Animation))\n                .map_err(ExecutionSessionInputError::Interaction),', '''        // Historical readers select Indicate from their displayed compiled row,
        // never the producer's current binding or semantic arena.
        if let Some(indication) = self.runtime.click_indicate_binding(object) {
            return self.runtime
                .prepare_click_indicate(object, indication)
                .map(|animation| animation.map(PreparedClickAction::Animation))
                .map_err(ExecutionSessionInputError::Interaction);
        }
        let Some(binding) = self.interaction_bindings.bindings.get(&node) else {
            return Ok(None);
        };
        match *binding {
            SemanticClickAction::Indicate(_) => unreachable!("Indicate lives in compiled history"),''')
(W/p).write_text(t)
resolve('web/execution-worker-client.js',lambda i,a,b: b+'    }\n    if (this.#semanticPreparedPlayback) {\n      throw new Error("prepared playback retains its historical player; use a fresh Run to replace it");\n' if i==1 else a)
resolve('web/main.js',lambda i,a,b:b)
replace('web/main.js','''        playerNeedsRestart = false;
        status.dataset.playbackPlaying = "false";''','''        if (isPreparedPlayback() || preparedPlayback) {
          discardEarlyContinuationRuntime(retainedPlayer);
        }
        playerNeedsRestart = false;
        status.dataset.playbackPlaying = "false";''')
resolve('web/semantic-engine-endpoint.js',lambda i,a,b: ('  if (prepared === null && (typeof context?.createExecutionPlayer !== "function" ||\n      typeof context.returnExecutionPlayer !== "function" ||\n      typeof context.discardExecutionPlayer !== "function" ||\n      typeof context.retireExecutionSource !== "function")) {\n' if i==0 else a.replace('let transport;','let transport = null;') if i==1 else ''))
p='web/semantic-engine-endpoint.js'
replace(p,'    if (player !== null) {\n      if (pendingPhaseJson !== null) {','    if (prepared === null && player !== null) {\n      if (pendingPhaseJson !== null) {')
replace(p,'    if (retireSource) context.retireExecutionSource();','    if (retireSource && prepared === null) context.retireExecutionSource();')
replace(p,'    controls.length = 0;\n    latestTick = null;\n    transport?.close?.();','''    controls.length = 0;
    latestTick = null;
    if (prepared !== null) {
      const retiringPlayer = player;
      player = null;
      // Cancellation precedes reader disposal; a trapped WASM context is never re-entered.
      try { onStop(); }
      catch (error) { notePreparedTrap(error); throw error; }
      finally {
        try { if (!engineTrapped) retiringPlayer?.stopSourcePreparation(); }
        catch (error) { notePreparedTrap(error); throw error; }
        finally {
          try { if (!engineTrapped) retiringPlayer?.free(); }
          catch (error) { notePreparedTrap(error); throw error; }
          finally {
            transport?.close?.();
            transport = null;
            renderPort.close();
            controlPort.close();
          }
        }
      }
      return;
    }
    transport?.close?.();''')
resolve('crates/noon-web/src/execution_canvas.rs',lambda i,a,b:a+b if i==0 else a)
p='crates/noon-web/src/execution_canvas.rs';t=(W/p).read_text()
t=t.replace('=> Ok(session),','=> Ok(session.session()),')
for method in ['has_audio','has_playback_controls','wake_state','frame','publication_context','inspection_view_revision','interactions_active','pointer_selection_presentation','painter_order','inset_2d_views','bounded_realtime_target','has_required_callbacks']:
 t=re.sub(r'(?<![.\w])session\.'+method+r'\(',f'session.session().{method}(',t)
t=t.replace('noon::diagnostics::execution_frame_value(session)','noon::diagnostics::execution_frame_value(session.session())')
t=t.replace('''                DirectSourceAuthority::Program(_) => None,
            }
        }

        fn static_execution_mut''','''                DirectSourceAuthority::Program(_) => None,
                #[cfg(feature = "prepared-prefix")]
                DirectSourceAuthority::Prepared(_) => None,
            }
        }

        fn static_execution_mut''')
t=t.replace('''                DirectSourceAuthority::Program(program) => program.retire_execution_source(),
            }''','''                DirectSourceAuthority::Program(program) => program.retire_execution_source(),
                // A reader has no mutable semantic arena or temporary GlowPulse attachment.
                #[cfg(feature = "prepared-prefix")]
                DirectSourceAuthority::Prepared(_) => Ok(()),
            }''')
(W/p).write_text(t)

r=R;p='web/playground-preview-rerun.test.mjs'
t=(r/'pr2308'/p).read_text()
t=t.replace('pause','retire')
t=t.replace('''  const oldPlayer = {''','''  const temporaryAttachment = {};
  let activeAttachment = started && !prepared ? temporaryAttachment : null;
  const oldPlayer = {''')
t=t.replace('    retire() {','    retireExecutionSource() {')
t=t.replace('return retire().then(() => { playing = false; });','return retire().then(() => { playing = false; activeAttachment = null; });')
t=t.replace('''    events.push("author"); authorCalls += 1;''','''    assert.equal(activeAttachment, null, "new source must not mutate the arena before input retirement");
    events.push("author"); authorCalls += 1;''')
t=t.replace('''    get authorCalls() { return authorCalls; }, get releases() { return releases; },''','''    get authorCalls() { return authorCalls; }, get releases() { return releases; },
    get activeAttachment() { return activeAttachment; }, temporaryAttachment,''')
a=t.index('for (const synchronous of [false, true]) {');b=t.index('for (const reject of [false, true]) {',a)
mt=(r/'master'/p).read_text();ma=mt.index('for (const synchronous of [false, true]) {');mb=mt.index('for (const reject of [false, true]) {',ma)
t=t[:a]+mt[ma:mb]+t[b:]
t=t.replace('["hold", "terminate", "prepare", "author", "hold"]','["hold", "retire", "terminate", "prepare", "author", "hold"]')
t+='''\nfor (const prepared of [false, true]) {
  test(`${prepared ? "prepared" : "ordinary"} retirement failure prevents prepared replacement and retains its exact owner`, async () => {
    const rejection = new Error("source cleanup rejected");
    const f = fixture({ prepared, retire: () => Promise.reject(rejection),
      authoringRunOptions: () => ({ preparedPlayback: true }) });
    const result = await f.context.runScene();
    assert.equal(result.error, rejection);
    assert.deepEqual(f.events, ["hold", "retire", "hold"]);
    assert.equal(f.authorCalls, 0);
    assert.equal(f.oldPlayer.terminations, 0);
    assert.equal(f.context.player, f.oldPlayer);
    assert.equal(f.context.preparedPlaybackPlayer, prepared ? f.oldPlayer : null);
    assert.equal(f.activeAttachment, prepared ? null : f.temporaryAttachment);
    assert.deepEqual(f.errors, [rejection]);
  });
}
'''
(r/'work'/p).write_text(t)

r=W
def edit(p,old,new,n=1):
 p=r/p;t=p.read_text();assert t.count(old)==n,(str(p),t.count(old),old[:80]);p.write_text(t.replace(old,new))
edit('web/playground-prepared-lifecycle.test.mjs','      async restart() { calls.push(["restart"]);', '''      async retireExecutionSource() {
        calls.push(["retire-source", this]);
        this.status.playing = false;
      },
      async restart() { calls.push(["restart"]);''')
p=r/'web/playground-restart-integration.test.mjs';t=p.read_text();a=t.index('    test(`navigation during replay pause');b=t.index('\n    });',a)+len('\n    });');s=t[a:b].replace('replay pause','source retirement').replace('owner.pause =','owner.retireExecutionSource =').replace('after-preview-pause','after-source-retirement').replace('actual pause acknowledgement','actual retirement acknowledgement').replace('late pause rejection','late retirement rejection');t=t[:a]+s+t[b:];p.write_text(t)
edit('crates/noon/src/execution_session/replay/prefix.rs','            || self.has_playback_controls()','            || !self.interaction_bindings.is_empty()')
edit('crates/noon/src/execution_session/input.rs','            || self.has_playback_controls()','            || !self.interaction_bindings.is_empty()')
edit('crates/noon/src/execution_session/selection.rs','self.runtime.has_click_indicate_bindings() || self.has_playback_controls()','self.runtime.has_click_indicate_bindings() || !self.interaction_bindings.is_empty()')
edit('crates/noon/src/execution_session/replay/prefix/reader.rs','''        self.session
            .advance_interactions(wall_time)
            .map_err(|error| PrefixError::Interaction(error.to_string()))''','''        self.session.runtime
            .advance_interactions(wall_time)
            .map_err(PrefixError::Interaction)''')
edit('crates/noon/src/execution_session/replay/prefix/reader/input.rs','self.session.submit_native_pointer_input(token, input)','self.session.submit_prepared_pointer_presentation(token, input)')
# Reuse shared pointer preparation/commit, rejecting semantic actions before acknowledgement.
edit('crates/noon/src/execution_session/input.rs','''    /// Commit one typed native occurrence and a preflighted scoped effective
''','''    /// The historical reader admits only presentation effects. Preparation and
    /// metadata commit are the ordinary pointer lane; no producer arena is borrowed.
    #[cfg(feature = "prepared-prefix")]
    pub(super) fn submit_prepared_pointer_presentation(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        let effective = self.runtime.prepare_effective_property_batch(&[])
            .expect("an empty effective-property batch is always valid");
        let prepared = self.prepare_native_pointer_input_with_effective(token, input, effective, false)?;
        let (frame, effective, timeline, metadata) = prepared.into_parts();
        if frame.is_some() || effective.is_some() || timeline.is_some()
            || matches!(metadata.action,
                Some(super::interactions::PreparedClickAction::GlowPulse { .. }
                    | super::interactions::PreparedClickAction::Playback(_)))
        {
            return Err(ExecutionSessionInputError::Interaction(
                "prepared pointer input does not admit causal or semantic mutation".into(),
            ));
        }
        self.commit_prepared_native_pointer_metadata(None, metadata)
    }

    /// Commit one typed native occurrence and a preflighted scoped effective
''')

for f,expr in [('crates/noon-web/src/browser_pointer_input.rs','self.session()'),('crates/noon-web/src/semantic_execution_player/pointer_input.rs','&*self.session')]:
 p=r/f;s=p.read_text()
 for method in ['has_native_pointer_subscribers','translation_drag_active','inspection_camera']:
  old=f'BrowserPointerTarget::{method}({expr})';new=f'({expr}).{method}()';assert old in s;s=s.replace(old,new)
 old=f'BrowserPointerTarget::capture_pointer_frame({expr}, view)';assert old in s;s=s.replace(old,f'({expr}).capture_pointer_frame(view)')
 old=f'BrowserPointerTarget::validate_pointer_frame({expr}, frame, view, validation)';assert old in s
 s=s.replace(old,'''match validation {
            crate::browser_pointer_input::PointerFrameValidation::Input => frame.validate_current(SESSION, view),
            crate::browser_pointer_input::PointerFrameValidation::Presentation => frame.validate_presentation(SESSION, view),
            crate::browser_pointer_input::PointerFrameValidation::CapturedGesture => frame.validate_captured_gesture(SESSION, view),
        }'''.replace('SESSION',expr))
 old=f'BrowserPointerTarget::pointer_input_token_for_frame({expr}, frame, view)';assert old in s;s=s.replace(old,f'frame.input_token({expr}, view)')
 p.write_text(s)
p=r/'crates/noon/tests/prepared_prefix_bindings.rs';s=p.read_text()
s=s.replace('fn click(session: &mut ExecutionSession, sequence: u64) {','fn click(store: &mut noon_core::SemanticStore, session: &mut ExecutionSession, sequence: u64) {')
s=s.replace('session.submit_native_pointer_input(&token, input)','session.submit_native_pointer_input(store, &token, input)')
s=s.replace('''    drop(target);
    drop(scene);

    for (index, (time, binding))''','''    let semantic_owner = std::rc::Rc::clone(scene.integration_store());
    drop(target);
    drop(scene);

    for (index, (time, binding))''')
s=s.replace('click(&mut source, 2 * index as u64);','click(&mut semantic_owner.borrow_mut(), &mut source, 2 * index as u64);')
s=s.replace('source.advance_interactions(wall_time)', 'source.advance_interactions(&mut semantic_owner.borrow_mut(), wall_time)')
s=s.replace('.advance_interactions(wall_time + binding.run_time() * 0.5)\n                .unwrap();\n            midpoint(source', '.advance_interactions(&mut semantic_owner.borrow_mut(), wall_time + binding.run_time() * 0.5)\n                .unwrap();\n            midpoint(source')
s=s.replace('''            source
                .advance_interactions(wall_time + binding.run_time())''','''            source
                .advance_interactions(&mut semantic_owner.borrow_mut(), wall_time + binding.run_time())''')
s+='''
#[test]
fn prepared_history_rejects_live_glow_pulse_without_borrowing_a_semantic_owner() {
    let mut scene = Scene::new();
    let target = scene.circle(0.65).unwrap();
    scene.add(&target).unwrap();
    let pulse = scene.declare_glow_pulse(&target, 2.0, AnimationOptions::new().run_time(1.0)).unwrap();
    scene.on_click(&target, Some(&pulse)).unwrap();
    let source = scene.execution_session().unwrap();
    assert!(source.has_native_pointer_subscribers(), "ordinary live GlowPulse stays interested in input");
    assert!(!source.has_playback_controls(), "GlowPulse must not masquerade as a playback control");
    assert!(source.validate_prepared_source_domain().is_err());
    assert!(target.get_effect("glow").is_err(), "admission does not enroll a temporary attachment");
}
'''
p.write_text(s)
p=r/'docs/architecture.md';s=p.read_text();mark='The public prefix reader owns an ordinary `ExecutionSession`, not a second input\nengine.'
assert mark in s
s=s.replace(mark,'''Live GlowPulse and playback-control declarations remain with their canonical
semantic owner and are rejected by prepared-source admission. The historical
reader admits only compiled Indicate presentation effects: it reuses shared
pointer preparation and metadata commit, rejecting causal writes or a semantic
lifecycle action before acknowledging input. It never invents or borrows a
mutable SemanticStore. Ordinary live sources continue to retire temporary glow
attachments through their original SceneExecution/LiveProgram store-session pair.

'''+mark)
p.write_text(s)

p = W / 'web/playground-restart-integration.test.mjs'
s = p.read_text()
old = 'assert.equal(result.stage, "after-source-retirement");'
assert s.count(old) == 1
s = s.replace(old, 'assert.equal(result.stage, outcome === "resolve" ? "after-source-retirement" : "error");')
p.write_text(s)
p = W / 'web/prepared-authoring-worker.test.mjs'
s = p.read_text()
old = '    prepareExecutionRun() {}, liveHandoffDuration: () => 1,'
assert s.count(old) == 1
s = s.replace(old, old + '\n    retireExecutionSource() { events.push("retire-source"); },')
p.write_text(s)
p = W / 'crates/noon-web/src/execution_canvas.rs'
s = p.read_text()
s = s.replace('(session.session().wake_state().frame_pending(), camera)', '(session.wake_state().frame_pending(), camera)')
p.write_text(s)
print('Reconciled source candidate; caller must verify the exact resulting tree before compiling.')
