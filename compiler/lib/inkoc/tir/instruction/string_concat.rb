# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class StringConcat
        include Predicates
        include Inspect

        attr_reader :register, :start, :length, :location

        def initialize(register, start, length, location)
          @register = register
          @start = start
          @length = length
          @location = location
        end

        def visitor_method
          :on_string_concat
        end
      end
    end
  end
end
