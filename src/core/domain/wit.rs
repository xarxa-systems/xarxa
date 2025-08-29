

pub struct RunId(u64);

pub struct History<'a, T> {
    steps: Vec<Step<'a, T>>
}

pub struct Step<'a, T> {
    pub name: &'a str,
    pub value: T,
}
