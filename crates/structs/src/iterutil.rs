use arraydeque::ArrayDeque;

pub struct KLookahead<I, const K: usize>
where
    I: Iterator,
{
    iterator: I,
    buffer: ArrayDeque<I::Item, K>,
}

impl<I, const K: usize> KLookahead<I, K>
where
    I: Iterator,
{
    pub fn new(it: I) -> Self {
        Self {
            iterator: it,
            buffer: ArrayDeque::new(),
        }
    }

    fn enqueue(&mut self) -> bool {
        if let Some(item) = self.iterator.next() {
            self.buffer.push_back(item).unwrap();
            true
        } else {
            false
        }
    }

    pub fn peek(&mut self, ahead: usize) -> Option<&I::Item> {
        while self.buffer.len() <= ahead {
            if !self.enqueue() {
                return None;
            }
        }

        self.buffer.get(ahead)
    }
}

impl<I, const K: usize> Iterator for KLookahead<I, K>
where
    I: Iterator,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.buffer.pop_front() {
            Some(item)
        } else {
            self.enqueue();
            self.buffer.pop_front()
        }
    }
}
