//
//  ViewportFrame.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/10/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI

fileprivate let minPadding: CGFloat = 10
fileprivate let botPadding: CGFloat = 0

struct ViewportFrame<Content>: View where Content: View {
    @EnvironmentObject var environment: MonocurlEnvironment
    
    let size: CGSize
    let cache: ViewportCache // (w/h)
    let content:  Content
    
    init(size: CGSize, cache: ViewportCache, @ViewBuilder content: () -> Content) {
        self.size = size
        self.cache = cache;
        self.content = content()
    }

    static func internal_rect(size: CGSize, aspectRatio: CGFloat) -> CGRect {
        let width = min(size.width - 2 * minPadding, (size.height - 2 * minPadding - botPadding) * aspectRatio)
        let height = width / aspectRatio
        
        return CGRect(x: size.width / 2 - width / 2, y: size.height/2 - height / 2, width: width, height: height)
    }
    
    var sub: Path {
        Rectangle().path(in: Self.internal_rect(size: self.size, aspectRatio: self.cache.aspectRatio))
    }
    
    var borderMask: some View {
        var full = Rectangle().path(in: CGRect(origin: CGPoint(x: 0, y: 0), size: self.size))
        full.addPath(self.sub)
        
        return full.fill(style: FillStyle(eoFill: true))
    }
    
    var body: some View {
        ZStack {
            Rectangle().foregroundColor(.black)
            
            self.content
            
            Rectangle()
                .fill()
                .foregroundColor(self.frameColor)
                .animation(Animation.easeIn(duration: 0.1), value: self.strokeColor)
                .mask(self.borderMask)
            
            //border
            if !environment.inPresentationMode {
                self.sub
                    .stroke()
                    .foregroundColor(self.strokeColor)
                    .animation(Animation.easeIn(duration: 0.1), value: self.strokeColor)
            }
        }
    }
    
    private var frameColor: Color {
        if environment.inPresentationMode {
            return .black
        }
        else {
            return .gray.opacity(0.6);
        }
    }
    
    private var strokeColor: Color {
//        if self.environment.inPresentationMode {
//            return .black
//        }
        
        switch(self.cache.viewportState) {
        case VIEWPORT_PLAYING:
            return .white;
        case VIEWPORT_IDLE:
            return .gray
        case VIEWPORT_COMPILER_ERROR, VIEWPORT_RUNTIME_ERROR:
            return .red
        case VIEWPORT_LOADING:
            return .blue
        default:
            return .white
        }
    }
}
