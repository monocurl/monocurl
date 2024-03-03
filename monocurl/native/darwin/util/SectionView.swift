//
//  SectionView.swift
//  Monocurl
//
//  Created by Manu Bhat on 12/19/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI


struct SectionView: View {
    @Binding var section: String
    @Binding var url: URL?
    
    let sections: [String]
    
    private func button(_ label : String) -> some View {
        VStack {
            if (self.section == label) {
                Text(label)
                    .frame(width: 80)
                    .foregroundColor(.black)
                    .font(.body)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 5)
                    .polyBackground(
                        RoundedRectangle(cornerRadius: 5)
                            .fill(.yellow)
                    )
            }
            else {
                Button {
                    self.section = label
                } label: {
                    Text(label)
                        .frame(width: 80)
                        .font(.body)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 5)
                        .contentShape(Rectangle())
                        .polyBackground(
                            RoundedRectangle(cornerRadius: 5)
                                .stroke(.yellow, lineWidth: 2)
                        )
                }
                .buttonStyle(.plain)
            }
        }
    }
    
    var body: some View {
        HStack {
            Spacer()
            
            Button("Home") {
                url = nil
            }
            .buttonStyle(.link)
            
            ForEach(self.sections, id: \.self) {
                self.button($0)
            }
            
            Spacer()
        }
        .padding()
    }
}

struct SectionView_Previews: PreviewProvider {
    static var previews: some View {
        SectionView(section: .constant("Label A"), url: .constant(nil), sections: ["Label A", "Label B", "Label C"])
    }
}
