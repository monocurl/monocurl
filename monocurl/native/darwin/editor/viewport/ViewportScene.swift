//
//  Mobjects.swift
//  Monocurl
//
//  Created by Manu Bhat on 10/26/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI
import Metal
import MetalKit

//just using metal for now, shouldn't be too hard
//ig just have a map of mesh to buffer and index
//and then just reuse that
//ok yeah lets just do metal

//so we have camera, and then ordered meshes, which provides all the info we really need

struct ViewportScene: NSViewRepresentable {
    
    let state: ViewportCache
  
    func makeCoordinator() -> ViewportMTKViewDelegate {
        return ViewportMTKViewDelegate()
    }
    
    func makeNSView(context: Context) -> MTKView {
        let mtkView = MTKView();
        
        mtkView.device = AppDelegate.gpu
        context.coordinator.initialize(gpu: mtkView.device!, cache: state);
        mtkView.delegate = context.coordinator;
        mtkView.sampleCount = mtkView.device!.supportedSampleCount
        mtkView.depthStencilPixelFormat = .depth32Float
        mtkView.clearDepth = 1.0
        mtkView.isPaused = true
        mtkView.enableSetNeedsDisplay = true
        
        return mtkView;
    }
    
    func updateNSView(_ mtkView: MTKView, context: Context) {
        context.coordinator.cache = self.state;
        context.coordinator.initializeProjections();
        mtkView.setNeedsDisplay(mtkView.frame)
    }
}
