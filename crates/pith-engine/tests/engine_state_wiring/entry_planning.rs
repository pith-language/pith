use super::*;

#[test]
fn entry_planning_drives_to_the_first_action_without_executing_it() {
    let action_interface = Interface {
        inputs: Box::new([]),
        output: Type::Blob,
    };
    let entry_interface = action_interface.clone();
    let mut engine = engine_with_fixtures();
    register_action_fixtures(&mut engine, &action_interface, &entry_interface);

    let request = pure_request("entry", entry_interface, []);
    let planned = match engine.plan_entry(&request) {
        Ok(planned) => planned,
        Err(diagnostics) => unreachable!("entry planning failed: {diagnostics:?}"),
    };

    assert_eq!(planned.request.interface, action_interface);
    assert_eq!(
        planned.action.spec.executable.host_path(),
        Some(action_executable())
    );
    assert!(engine.query().computations().all(|(_, node)| {
        !matches!(&node.kind, ComputationKind::Action(_))
            && matches!(&node.state, AttemptState::Cancelled { .. })
    }));
}
