//
//  Polyfills.swift
//  Monocurl
//
//  Created by Manu Bhat on 2/20/23.
//  Copyright © 2023 Enigmadux. All rights reserved.
//

import Foundation
import SwiftUI


extension View {
    @ViewBuilder
    func polyOverlay(_ color: Color) -> some View {
        self.overlay(color)
    }
    
    @ViewBuilder
    func polyBackground<Background: View>(_ view: Background) -> some View {
        self.background(view)
    }
}
