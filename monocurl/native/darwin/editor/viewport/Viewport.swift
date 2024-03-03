//
//  Viewport.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/9/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI

//same as monocurl swift, just have viewing window and what not
//should be possible to do with scene kit now

#warning("TODO, presentation mode should be fixed with cocoa update")
struct Viewport: View {
    @State private var state: ViewportCache
   
    init(_ ref: UnsafeMutablePointer<viewport>) {
        self._state = State(initialValue: ViewportCache(ref, nonce: 0))
    }
    
    var body: some View {
        GeometryReader { geometry in
            ViewportFrame(size: geometry.size, cache: state) {
                ViewportScene(state: self.state)
            }
        }
        .frame(minWidth: 500, idealWidth: 700, maxWidth: .infinity,
                minHeight: 250, idealHeight: 300, maxHeight: .infinity)
        .onAppear {
            viewportStore = $state
            self.state = ViewportCache(self.state.ref, nonce: 1)
        }
        .onDisappear {
            viewportStore = nil;
        }

    }
}
